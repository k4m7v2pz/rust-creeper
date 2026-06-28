# MOTD 查询网络错误诊断手册

> 为什么查不到 Minecraft 服务器 MOTD？从 DNS 到协议层，完整排查路径与原因对照。
>
> 实测数据来源：lambdaattack 项目对 9 个国内社区服务器的真实探测记录。

---

## 目录

1. [总体诊断路径](#1-总体诊断路径)
2. [DNS 层](#2-dns-层)
3. [TCP 传输层](#3-tcp-传输层)
4. [Minecraft 协议层](#4-minecraft-协议层)
5. [本地环境层](#5-本地环境层)
6. [实测案例](#6-实测案例)

---

## 1. 总体诊断路径

```
发起 MOTD 查询
    │
    ▼
┌─────────────────┐
│  DNS 解析        │ ← 域名 → IP
└──────┬──────────┘
       │
       ▼
┌─────────────────┐
│  TCP 三次握手    │ ← 连接目标端口
└──────┬──────────┘
       │
       ▼
┌─────────────────┐
│  Minecraft 握手  │ ← SLP (Server List Ping) 协议
└──────┬──────────┘
       │
       ▼
┌─────────────────┐
│  解析 MOTD JSON  │ ← 获取 description / players / version
└─────────────────┘
```

任何一步失败，MOTD 就拿不到。错误可以从上层向下逐层定位。

---

## 2. DNS 层

### 2.1 NXDOMAIN — 域名不存在

**现象**: `DNS resolution failed` / `Temporary failure in name resolution`

**原因**:
- 域名已过期未续费
- 域名被删除或未注册
- 拼写错误（typo）

**实测**: `server-a.example.com`、`server-b.example.net`、`server-c.example.org` 均为此类。域名本身已不存在于 DNS 系统中。

**排查**: `dig +short <domain>` / `nslookup <domain>`

**状态**: ✅ 已遇到（3 次）

---

### 2.2 DNS 服务器超时

**现象**: `connection timed out; no servers could be reached`

**原因**:
- 本地 DNS 服务器宕机
- 上游 DNS 递归超时
- 跨国 DNS 查询被阻断

**排查**: 换公共 DNS（`8.8.8.8` / `1.1.1.1`）重试

**状态**: ❌ 未遇到

---

### 2.3 A/AAAA 记录指向错误 IP

**现象**: 域名能解析，但连上的是错误主机

**原因**:
- DNS 记录被篡改（DNS 劫持）
- CDN 未配置正确回源
- 旧记录未清理（IP 已变更）

**排查**: `dig <domain>` 查看实际 IP，对比预期

**状态**: ❌ 未遇到

---

### 2.4 SRV 记录缺失

**现象**: 使用 `_minecraft._tcp.<domain>` SRV 记录的连接方式失败

**原因**:
- 部分社区服使用 SRV 将端口重定向到非标端口
- SRV 记录配置错误或已删除

**状态**: ❌ 未遇到

---

## 3. TCP 传输层

### 3.1 Connection Refused — 连接被拒绝

**现象**: `Connection refused`

**原因**:
- 目标端口上**没有进程在监听**
- 服务端已关闭但机器还在线
- 端口号不对（填错了或服务端改了端口）
- 服务端只监听内网 IP（`127.0.0.1` / `0.0.0.0` 但防火墙拦截）

**排查**:
```bash
nc -zv <host> <port>     # 测试端口是否开放
nmap -p <port> <host>     # 扫描端口状态
```

**实测**: `play.example.com:25565`、`07f4ac.example.net:55673`、`rat.example.ci` 均为此类。域名能解析到 IP，但目标端口没人监听。

**状态**: ✅ 已遇到（4 次，最常见）

---

### 3.2 Connection Timed Out — 连接超时

**现象**: `connection timed out`

**原因**:
- 目标服务器**已关机**（物理机 / VPS 离线）
- 中间防火墙**静默丢弃** TCP SYN 包（不返回 RST，也不响应）
- 跨国路由丢包严重，超出 TCP 重试次数
- 骨干网 BGP 断线
- ICMP 被屏蔽但 TCP 也不同（可能被 DDoS 防护拦截）

**排查**:
```bash
ping <host>               # 测 ICMP 是否通
mtr -r <host>             # 看路由在哪跳丢包
traceroute <host>         # 追踪路由路径
```

**实测**: `198.51.100.1:2843`、`server-d.example.top:25565` 均超时。IP 可达但端口无响应，大概率是防火墙静默丢弃或服务器已关机。

**状态**: ✅ 已遇到（2 次）

---

### 3.3 No Route to Host — 路由不可达

**现象**: `No route to host`

**原因**:
- 目标网段从当前网络出发不可达
- BGP 路由被撤销
- 本地路由表错误

**状态**: ❌ 未遇到

---

### 3.4 Network is Unreachable — 网络不可达

**现象**: `Network is unreachable`

**原因**:
- 本地无网络连接
- 网关宕机
- VPN 断开导致路由不可达

**状态**: ❌ 未遇到

---

### 3.5 Connection Reset — 连接被重置

**现象**: `Connection reset by peer`

**原因**:
- 中间防火墙 / IPS 主动发送 RST 阻断
- 服务端连接数满，主动拒绝
- TCP 存活检测失败

**状态**: ❌ 未遇到

---

## 4. Minecraft 协议层

（TCP 连接成功，但 MOTD 仍然拿不到的情况）

### 4.1 协议版本不兼容

**现象**: 连接成功，服务端返回错误或直接断连

**原因**:
- 客户端发送的 `protocol_version` 与服务端不匹配
- 服务端反代（BungeeCord / Velocity）要求特定版本
- 服务端 mod（如 ViaVersion）未正确配置

**对应**: `gamedig` 默认使用 `protocol_version=-1`（自动适配大多数情况），但如果服务端强制校验版本号则会失败。

**状态**: ❌ 未遇到

---

### 4.2 服务端禁用了 Server List Ping

**现象**: TCP 连接成功，握手正常，但发送 Status Request 后无响应

**原因**: `server.properties` 中 `enable-status=false` — 常见于安全加固后的服务器，有意隐藏 MOTD

**状态**: ❌ 未遇到

---

### 4.3 反代后端 — 直连后端拿不到 MOTD

**现象**: 连接后端真实 IP 端口成功，但 get 不到 MOTD

**原因**:
- 服务器位于 BungeeCord / Velocity / Waterfall 后端
- 后端服务器本身不处理 SLP 请求（代理才处理）
- 需要连代理端口，而非后端端口

**状态**: ❌ 未遇到（但常见于大型社区服）

---

### 4.4 极老版本（1.6 之前）

**现象**: 使用现代 SLP 协议查询无响应

**原因**: 1.6 版本之前使用不同的 ping 机制（基于 `0xFE` packet），`gamedig` 默认使用 1.7+ 协议

**状态**: ❌ 未遇到

---

### 4.5 需要特定 Hostname 才能响应

**现象**: 用 IP 直连拿不到 MOTD，用域名才能拿到

**原因**: 虚拟托管服务器（如 Hypixel 类的群组服）根据握手时的 `hostname` 字段路由到不同后端。如果传来的是 IP 而不是域名，后端不识别，不会返回 MOTD。

**对应**: `gamedig` 的 `RequestSettings.hostname` 参数就是为此设计。

**状态**: ❌ 未遇到

---

### 4.6 返回数据格式异常

**现象**: TCP 连接成功，服务端返回了数据，但 JSON 解析失败

**原因**:
- MOTD description 包含非法字符（非 UTF-8）
- favicon 字段损坏
- 自定义插件篡改了 SLP 返回格式

**状态**: ❌ 未遇到

---

### 4.7 被限流 / IP 封禁

**现象**: 前几次能查到 MOTD，连续查几次后突然超时或被拒

**原因**: 服务器有频率限制保护（如 `connection-throttle`），短时间内大量 ping 请求触发封禁

**状态**: ❌ 未遇到

---

### 4.8 SSL / TLS 干扰

**现象**: TCP 连接被重置或超时，没有明显的应用层数据

**原因**:
- 服务器使用了 Cloudflare / CDN 防护
- CDN 尝试进行 SSL 握手，但 Minecraft 发送的是原生 TCP 数据包
- CDN 无法识别 SLP 流量，直接断开

**状态**: ❌ 未遇到（但推测部分使用 Cloudflare 的 MC 服务器会遇到）

---

## 5. 本地环境层

### 5.1 本地出站防火墙

**现象**: 所有服务器的特定端口都连不上

**原因**: 公司/学校/运营商网络屏蔽了 Minecraft 端口

**排查**: `nc -zv 8.8.8.8 53`（正常） vs `nc -zv <any-mc-server> 25565`（超时）

**状态**: ❌ 未遇到

---

### 5.2 代理 / VPN 干扰

**现象**: 通过代理时连接失败，直连正常

**原因**:
- SOCKS5 代理未正确处理 TCP 流
- HTTP 代理无法代理非 HTTP 流量
- VPN 路由策略导致流量未正确转发

**状态**: ❌ 未遇到

---

### 5.3 IPv6 优先导致失败

**现象**: 域名有 AAAA 记录但服务端只监听 IPv4

**原因**: 现代操作系统默认 IPv6 优先，连接 `AAAA` 返回的 IP 失败后，不会回退到 IPv4

**排查**: `curl -4` vs `curl -6` / `dig A <domain>` vs `dig AAAA <domain>`

**状态**: ❌ 未遇到

---

### 5.4 MTU 分片问题

**现象**: 小包能通（TCP SYN），大包被丢弃（握手数据包超过 MTU）

**原因**: 跨国线路或 VPN 隧道 PMTUD 失效

**状态**: ❌ 未遇到

---

## 6. 实测案例

| # | 服务器 | 结果 | 根因 | 分类 |
|---|--------|------|------|------|
| 1 | `server-a.example.com:25565` | DNS 解析失败 | 域名过期/不存在 | DNS |
| 2 | `play.example.com:25565` | Connection refused | 端口无监听 | TCP |
| 3 | `origin.example.com:21996` | ✅ **成功** | – | – |
| 4 | `198.51.100.1:2843` | Timed out | 防火墙静默丢弃/关机 | TCP |
| 5 | `server-b.example.net:25565` | DNS 解析失败 | 域名过期/不存在 | DNS |
| 6 | `server-c.example.org:25565` | DNS 解析失败 | 域名过期/不存在 | DNS |
| 7 | `server-d.example.top:25565` | Timed out | 防火墙静默丢弃/关机 | TCP |
| 8 | `07f4ac.example.net:55673` | Connection refused | 端口无监听 | TCP |
| 9 | `rat.example.ci:25565` | Connection refused | 端口无监听 | TCP |

### 统计

| 失败原因 | 次数 | 占比 |
|----------|------|------|
| DNS 域名不存在 | 3 | 33% |
| TCP Connection Refused | 4 | 44% |
| TCP Connection Timed Out | 2 | 22% |
| **成功** | **1** | **11%** |

9 个社区服务器中，**8 个（89%）已失联**，仅 1 个仍在运行。这说明社区 Minecraft 服务器的生命周期非常短暂——域名过期、关服、端口变更都极为常见。motd 查询本身就是一种"服务器健康检查"。
