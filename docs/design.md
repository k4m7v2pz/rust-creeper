# Creeper 设计文档

## 项目定位

跨平台 Minecraft 压力测试机器人。运行在 VPS / 服务器上，提供 CLI（给 agent 用）、TUI（给人用）、REST API（给 WebUI 用）三种交互方式。

## 架构概览

```
用户/Agent ──→ CLI (clap) ──→ Core ──→ Protocol ──→ Minecraft Server
                                       │
                                       └── Proxy Layer (SOCKS4/5, HTTP)
```

| 层 | 职责 | 技术 |
|---|---|---|
| **CLI** | 参数解析、配置管理、服务管理 | clap, daemon-kit |
| **Config** | JSON 配置加载/保存/校验 | serde, serde_json, JSON Schema |
| **Core** | 攻击编排、Bot 生命周期 | tokio, mpsc channel |
| **Protocol** | Minecraft 协议抽象 | trait-based, 版本无关 |
| **Proxy** | 代理隧道连接 | proxied (SOCKS4/5, HTTP CONNECT) |

## 协议版本支持

当前 26 个版本，来自：

- **TuxCoding fork**: 1.11 ~ 1.16.5（旧版 Steveice10 MCProtocolLib，每个版本独立分包）
- **GeyserMC/MCProtocolLib**: 1.17.1 ~ 1.21.7（统一 MinecraftCodec 架构，按 tag 区分）

详见 `reference/` 目录下的参考实现。

## Forge 支持（待实现）

Forge 握手分为两套：

- **V1（1.7 ~ 1.12.2）**: 通过 `FML|HS` PluginMessage 通道，在连接后协商
- **V2（1.13+）**: 通过 `fml:loginwrapper` LoginPluginRequest/Response，在登录阶段协商

参考实现：`reference/endmc-ref/AdvanceModule/MCForge/`

## 配置

- 文件：`~/.config/creeper/config.json`
- Schema: `config/schema.json`
- 所有字段有默认值，CLI 参数可覆盖

## 代理

支持三种协议，统一格式 `protocol://[user:pass@]host:port`：

| 协议 | 示例 |
|---|---|
| SOCKS4 | `socks4://127.0.0.1:1080` |
| SOCKS5 | `socks5://user:pass@127.0.0.1:1080` |
| HTTP CONNECT | `http://user:pass@127.0.0.1:8080` |

**兼容裸地址**：旧格式 `127.0.0.1:1080` 会自动补为 `socks5://127.0.0.1:1080`。

代理验证方式：通过代理向目标 MC 服务器发起 TCP 连接 + ServerListPing 握手，能收到响应即为可用（配置 `proxy.verify: true`）。

## 重试机制

某些服务器有防机器人插件，首次进服会被踢。配置 `bot.max_attempts` 控制重连次数：

- `max_attempts: 1`（默认）— 不重试
- `max_attempts: 3` — 被踢后自动重新连接，最多 3 次
- 重连之间间隔 1 秒
- 用户主动 `disconnect` 不会触发重试

## 服务管理

| 平台 | 后端 |
|---|---|
| Linux | systemd user unit |
| macOS | launchd plist |
| Windows | Windows Service (SCM) |

安装后自动随系统启动，`creeper service run` 为前台模式供服务管理器调用。

## 反机器人绕过（待实现）

参考 `reference/endmc-ref/AdvanceModule/ACProtocol/`：

- **重试机制**: 配置 `max_attempts`，被踢后换代理重连
- **AntiCheat3**: 自定义数据包/时间戳绕过
- **CatAntiCheat**: 猫反作弊协议处理
- **AnotherStarAntiCheat**: AnotherStar 反作弊协议处理

---

## Hub 分布式架构

```
┌──────────────────────────────────────────────────┐
│                    Hub Server                     │
│  (hub-serve / axum REST API)                     │
│                                                   │
│  GET  /           → 节点元信息                    │
│  GET  /journal    → 玩家名册（多服聚合）           │
│  POST /journal    → 接收 monitor 上报              │
│  GET  /host-report → 探针资源数据                  │
│  POST /host-report → 接收 host-monitor 上报        │
│  POST /tasks      → 提交扫描任务                   │
│  GET  /tasks      → Worker poll 任务               │
│  PUT  /tasks/:id  → Worker 更新任务状态            │
│  POST /backup     → 拉取远程备份                   │
└──────────┬───────────────────────────┬────────────┘
           │                           │
           ▼                           ▼
    ┌─────────────┐           ┌──────────────┐
    │  Monitor     │           │  Worker      │
    │  (mc-monitor)│           │  (hub-worker)│
    │  探测 MC 服  │           │  poll 并执行  │
    │  sync 玩家   │           │  扫描/发现任务 │
    └─────────────┘           └──────────────┘
           │
           ▼
    ┌─────────────┐
    │ Host Monitor │
    │ (host-monitor)│
    │ 上报 CPU/RAM │
    │ 磁盘/负载    │
    └─────────────┘
```

| 角色 | 命令 | 职责 |
|------|------|------|
| **Hub** | `hub-serve` | 中心节点，聚合监控数据、分发任务 |
| **Monitor** | `mc-monitor` | 探测 MC 服务器在线状态 + 玩家名册，sync 到 Hub |
| **Worker** | `hub-worker` | 从 Hub poll 任务并执行（discover/scan） |
| **Probe** | `host-monitor` | 仅监控本机资源（CPU/RAM/磁盘），不上报 MC 数据 |

### 探针 vs 扫描节点分离

- **探针角色**（`host-monitor`）：仅监控在线/资源占用，**不得**派发扫描/攻击任务
- **Worker 角色**（`hub-worker`）：从 Hub poll 并执行 `discover`/`scan`/`crawl` 等任务

---

## 攻击链模块

攻击链按实际操作阶段分为三个模块，见 `src/attack_chain.rs` 实现：

| 模块 | 命令 | 文件 | 安全机制 |
|------|------|------|----------|
| **持久化后门投放** | `persist` | `run_persist()` | 默认 dry-run，`--execute` + 确认后执行 |
| **敏感文件回收** | `harvest` | `run_harvest()` | 只读操作，无破坏性 |
| **自毁抹盘** | `wipe` | `run_wipe()` | 默认 dry-run，`--execute` + 确认后执行 |

平台支持矩阵：

| 模块 | Linux | Windows | macOS |
|------|-------|---------|-------|
| `persist` | SSH key / crontab / systemd / bashrc | SSH key / schtasks / registry Run / Startup | ❌ 不支持 |
| `harvest` | SSH / shadow / cloud creds / history | SSH / cmdkey / PS history | ❌ 不支持 |
| `wipe` | dd / shred / rm -rf | diskpart / Clear-Disk / rd | ❌ 不在目标范围 |

---

## HIDS 主机入侵检测

基于攻击链路径反用，做只读安全检测，见 `src/hids.rs`：

| 检查项 | 实现函数 | 对应攻击路径 |
|--------|----------|-------------|
| authorized_keys 异常检测 | `check_ssh_authorized_keys()` | SSH 后门路径 |
| bash_history 清空痕迹 | `check_shell_history()` | history 路径清单 |
| 可疑 systemd 服务 / cron 任务 | `check_systemd_services()` + `check_cron_jobs()` | 持久化路径 |
| /dev/shm 可执行检测 | `check_shm_executables()` | 临时盘清单 |
| 云凭据泄漏检测 | `check_cloud_credentials()` | AWS/GCloud/Azure 路径 |
| 启动脚本后门检测 | `check_startup_scripts()` | bashrc/profile 路径 |
| 弱口令自检 | `check_weak_passwords()` | 弱口令爆破思路 |

---

## C2 通信（教育用途）

`c2-build` 命令构建 C2 agent 二进制：

- Agent 硬编码 C2 服务器地址
- 支持 HTTP/WS 正向连接与反向连接
- 支持心跳保活、任务下发、结果回传
- 详见 `src/c2core/` 模块

---

## 发现与扫描引擎

| 模块 | 命令 | 说明 |
|------|------|------|
| **域名模式发现** | `mc-discover` | 模板化域名 + 端口段扫描（如 `srv{}.example.com:1..50`） |
| **慢速爬取** | `mc-crawl` | 低并发长周期扫描，适合潜伏探索 |
| **端口扫描** | `port-scan` | IPv4 CIDR/范围 + 端口段，支持 TCP/UDP/HTTP/ICMP 等协议探测 |

---

## 服务管理

| 平台 | 后端 |
|---|---|
| Linux | systemd user unit |
| macOS | launchd plist |
| Windows | Windows Service (SCM) |

安装后自动随系统启动，`creeper service run` 为前台模式供服务管理器调用。
