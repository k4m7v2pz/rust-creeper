# Creeper · 苦力怕

> 侦查 · 监控 · 爆炸  
> Recon · Monitor · Attack

**Creeper** (网络苦力怕 / cyber creeper) 是一个多合一的 Minecraft 网络安全工具箱，灵感来自《我的世界》中的苦力怕（Creeper）——悄悄地接近，耐心地观察，然后 💥。

项目最初是 [LambdaAttack](https://github.com/games647/LambdaAttack) 的 Rust 移植，现已发展成包含以下三大乐趣的完整工具链：

---

## 🕵️ 侦查 — Recon / OSINT

侦察对手服务器的情报。

| 功能 | 说明 |
|------|------|
| `info` | 查询服务器 MOTD、在线玩家、版本、协议号、延迟等信息 |
| `check` | 检测域名是否在 CDN（Cloudflare 等）后面，获取真实 IP |
| 玩家日志 | 自动将查询到的玩家昵称、时间戳记录到本地 JSON 日志，供离线分析 |

```bash
creeper info mc.example.com
creeper check example.com
```

## 👁️ 监控 — Monitor

持续监视服务器动态，收集玩家行为数据。

| 功能 | 说明 |
|------|------|
| `monitor` | 定时 ping 服务器，记录玩家列表变化，追踪玩家在线/离线状态 |
| `tui` | 终端仪表盘（TUI），实时显示服务器状态、玩家日志增长、Flood 统计 |
| 玩家日志 | 每次扫描的玩家昵称自动存入 journal 文件，可积累长期数据 |

```bash
creeper monitor mc.example.com -t 30      # 每 30 秒扫描一次
creeper tui mc.example.com                 # 打开 TUI 仪表盘
```

## 💥 爆炸 — Attack / Stress-Test

对目标发起压力测试。

| 功能 | 说明 |
|------|------|
| `start` | Minecraft 机器人洪水攻击：批量登录假玩家，支持代理、自动注册、自定义昵称 |
| `http` | HTTP/HTTPS 洪水攻击：高并发请求，支持 GET/POST/PUT/DELETE 等所有方法，支持代理轮换 |

```bash
creeper start -h 127.0.0.1 -c 100 -d 500      # 100 个机器人，500ms 间隔
creeper http https://example.com -c 200 -n 10000   # 200 并发，共 10000 次请求
```

---

## 🚀 快速开始

### 环境要求

- Rust 1.75+
- Minecraft 1.21.1+ 服务器（使用 Minecraft 攻击功能时）

### 构建

```bash
cargo build --release
cargo test
```

### 基本用法

```bash
# 查看所有子命令
cargo run --release -- --help

# 启动 Minecraft 机器人攻击
cargo run --release -- start -h 127.0.0.1 -c 50

# 查询服务器信息
cargo run --release -- info mc.example.com

# 启动 TUI 仪表盘
cargo run --release -- tui mc.example.com

# HTTP 洪水攻击
cargo run --release -- http https://example.com -c 100
```

> 安装后可直接使用 `creeper` 命令（见下方安装说明）。

---

## 📦 安装

```bash
# 构建
cargo build --release

# 安装到系统路径
cp target/release/creeper /usr/local/bin/creeper   # macOS / Linux
# 或保留原名
cp target/release/creeper /usr/local/bin/creeper

# 注册系统服务（自启动）
creeper service install
creeper service start
creeper service status
```

---

## ⚙️ 配置

配置文件位于 `$XDG_CONFIG_HOME/creeper/config.json`（macOS: `~/Library/Application Support/creeper/config.json`）。

```bash
# 打印当前配置
creeper config --show

# 打印默认配置模板
creeper config --default

# 用 $EDITOR 编辑配置
creeper config --edit
```

完整配置说明见 [`config/schema.json`](config/schema.json)。

---

## 📚 文档

| 文档 | 说明 |
|------|------|
| [`docs/usage.md`](docs/usage.md) | 完整命令行参数说明 |
| [`docs/design.md`](docs/design.md) | 架构设计文档 |
| [`docs/attack-defense-notes.md`](docs/attack-defense-notes.md) | 攻击链头脑风暴与实现对照 |
| [`docs/TODO.md`](docs/TODO.md) | 开发计划 |
| [`docs/motd-network-errors.md`](docs/motd-network-errors.md) | MOTD 网络错误处理说明 |
| [`data/schema.json`](data/schema.json) | 玩家日志数据格式 |
| [`config/schema.json`](config/schema.json) | 配置文件格式 |
| [`data/targets-template.json`](data/targets-template.json) | 监控目标模板（示例，非实时数据） |

> 💡 **实时数据不在本地**：已发现的服务器、玩家名册等实时数据通过 Hub API 获取。  
> 详见 [`AGENTS.md`](AGENTS.md) 的"数据源架构"章节。

---

## 🔧 技术栈

- **语言**: Rust (edition 2021)
- **运行时**: Tokio (异步)
- **Minecraft 协议**: 支持 1.11 ~ 1.21.1 各版本
- **服务器查询**: gamedig（MOTD、玩家信息）
- **终端 UI**: Ratatui + Crossterm
- **代理支持**: SOCKS4/5 + HTTP（Minecraft 和 HTTP flood 均支持）
- **服务管理**: daemon-kit（支持 systemd / launchd / Windows Service）

---

## 📄 许可

**Unlicense** — 见 [LICENSE](LICENSE)。  
本项目已发布到公共领域。

引用的第三方项目的许可信息见 [`reference/README.md`](reference/README.md)。
