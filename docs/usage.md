# Creeper 使用指南

Minecraft 压力测试机器人。支持 SOCKS4/5、HTTP 代理，多版本协议（1.11 ~ 1.21.7），可用作 CLI 或系统服务。

## 目录

- [环境要求](#环境要求)
- [编译](#编译)
- [快速开始](#快速开始)
- [配置文件](#配置文件)
- [命令行参数](#命令行参数)
- [代理使用](#代理使用)
- [系统服务](#系统服务)
- [常见问题](#常见问题)

---

## 环境要求

- **Rust** 1.75+
- **操作系统**：Linux / macOS / Windows
- **网络**：能连接到目标 Minecraft 服务器

### 安装 Rust

如果你还没有 Rust：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

验证安装：

```bash
rustc --version
cargo --version
```

---

## 编译

```bash
# 克隆项目
git clone <项目地址>
cd rust-tokio-lambda-attack

# 编译（首次编译会下载依赖，可能需要几分钟）
cargo build --release

# 编译产物在
#   target/release/creeper
```

### 验证编译

```bash
# 查看帮助
./target/release/creeper --help

# 查看版本
./target/release/creeper --version
```

### 开发模式编译（更快，但性能差）

```bash
cargo build
# 产物在 target/debug/creeper
```

---

## 快速开始

### 1. 使用默认参数启动

```bash
cargo run --release -- start
```

这会在本地 `127.0.0.1:25565` 启动 20 个机器人（假设你本地有 MC 服务器）。

### 2. 指定目标服务器

```bash
cargo run --release -- start \
  --host mc.example.com \
  --port 25565 \
  --count 50 \
  --delay 200 \
  --version 1.21.1
```

参数说明：

| 参数 | 缩写 | 默认值 | 说明 |
|---|---|---|---|
| `--host` | `-h` | `127.0.0.1` | 服务器地址 |
| `--port` | `-p` | `25565` | 服务器端口 |
| `--count` | `-c` | `20` | 机器人数量 |
| `--delay` | `-d` | `1000` | 每个机器人加入间隔（毫秒） |
| `--name` | `-n` | `Bot-%d` | 机器人名字格式（`%d` 会被替换为序号） |
| `--version` | `-v` | `1.21.1` | Minecraft 版本 |
| `--register` | `-r` | — | 加入后自动执行 /register 和 /login |
| `--proxies` | `-P` | — | 代理列表文件路径 |
| `--nicks` | `-N` | — | 昵称列表文件路径 |
| `--config` | `-C` | — | 指定配置文件路径 |

### 3. 使用代理

先创建一个 `proxies.txt`：

```
socks5://127.0.0.1:1080
socks4://192.168.1.100:4145
http://user:password@proxy.example.com:8080
```

然后：

```bash
cargo run --release -- start \
  --host mc.example.com \
  --count 30 \
  --proxies proxies.txt
```

代理格式说明：

| 格式 | 协议 | 示例 |
|---|---|---|
| `socks5://host:port` | SOCKS5 | `socks5://127.0.0.1:1080` |
| `socks5://user:pass@host:port` | SOCKS5 带认证 | `socks5://user:pass@127.0.0.1:1080` |
| `socks4://host:port` | SOCKS4 | `socks4://10.0.0.1:4145` |
| `http://host:port` | HTTP CONNECT | `http://proxy.example.com:8080` |
| `host:port` | 自动补为 SOCKS5 | `127.0.0.1:1080`（兼容旧格式） |

---

## 配置文件

配置文件位于 `~/.config/creeper/config.json`，JSON 格式。

### 生成默认配置

```bash
cargo run --release -- config --default
```

这会打印默认配置到终端，你可以复制保存。

### 查看当前配置

```bash
cargo run --release -- config --show
```

### 编辑配置

```bash
cargo run --release -- config --edit
```

这会用 `$EDITOR` 或 `vim` 打开配置文件，保存后自动生效。

### 完整配置项

```jsonc
{
  "target": {
    "host": "127.0.0.1",        // 服务器地址
    "port": 25565,               // 服务器端口
    "version": "1.21.1",         // Minecraft 版本
    "count": 20,                 // 机器人数量
    "join_delay_ms": 1000        // 加入间隔（毫秒）
  },
  "bot": {
    "name_format": "Bot-%d",     // 名字格式
    "auto_register": false,      // 自动注册
    "join_commands": [],         // 加入后执行的命令列表
    "auto_respawn_delay_ms": -1, // 自动重生延迟（-1 = 禁用）
    "max_attempts": 1            // 最大连接尝试次数（防踢重连）
  },
  "proxy": {
    "list": [],                  // 代理列表
    "file": null,                // 代理文件路径
    "url": null,                 // 代理列表下载地址
    "verify": true               // 是否验证代理可用性
  },
  "nickname": {
    "realistic": false,          // 使用真实昵称列表
    "file": null,                // 昵称文件路径
    "length": 16,                // 随机昵称长度
    "prefix": ""                 // 昵称前缀
  },
  "server": {
    "ping_on_start": true        // 启动时 ping 服务器
  },
  "api": {
    "enabled": false,            // 启用 REST API
    "bind": "127.0.0.1",         // API 监听地址
    "port": 9720                 // API 端口
  }
}
```

### CLI 参数优先级

CLI 参数 > 配置文件 > 默认值。

示例：配置文件中设置了 `count: 50`，但命令行传了 `--count 100`，则实际使用 100。

---

## 系统服务

支持安装为系统服务，开机自启。

### Linux (systemd)

```bash
# 先编译
cargo build --release

# 安装服务
sudo ./target/release/creeper service install

# 启动
sudo ./target/release/creeper service start

# 查看状态
sudo ./target/release/creeper service status
# → ● creeper.service — running (PID 12345)

# 停止
sudo ./target/release/creeper service stop

# 卸载
sudo ./target/release/creeper service uninstall
```

### macOS (launchd)

同上命令，`daemon-kit` 会自动选择 launchd 后端。

### Windows (Service Manager)

同上，会自动注册为 Windows Service。

---

## 完整命令参考

### `creeper start`

启动攻击。

```bash
creeper start \
  -h mc.example.com \
  -p 25565 \
  -c 100 \
  -d 100 \
  -n "Player-%d" \
  -v 1.21.1 \
  -r \
  -P proxies.txt \
  -N nicks.txt
```

### `creeper config`

配置管理。

```bash
creeper config --default    # 打印默认配置
creeper config --show       # 查看当前配置
creeper config --edit       # 编辑配置文件
```

### `creeper service`

系统服务管理。

```bash
creeper service install     # 安装服务
creeper service uninstall   # 卸载服务
creeper service start       # 启动守护进程
creeper service stop        # 停止
creeper service status      # 查看状态
```

### `creeper info`

服务器信息（待实现）。

```bash
creeper info mc.example.com
```

---

## 代理文件格式

每行一个代理。支持 `socks4://`、`socks5://`、`http://` 协议，也兼容旧格式 `host:port`：

```
socks5://127.0.0.1:1080
socks4://10.0.0.1:4145
http://user:password@proxy.example.com:8080
# 以下会被自动补为 socks5://
192.168.1.1:9050
10.0.0.2:1080
```

空行和 `#` 开头的行会被忽略。

## 昵称文件格式

每行一个昵称：

```
Steve
Alex
Player123
BuildMaster
```

---

## 参考项目

参考实现位于 `reference/` 目录：

| 项目 | 用途 |
|---|---|
| `mcprotocollib/` | GeyserMC MCProtocolLib — Java MC 协议库 |
| `mc-bots-ref/` | crpmax/mc-bots — Java MC 机器人 |
| `endmc-ref/` | EndMinecraftPlusV2 — Java MC 压力测试，支持 Forge + 反作弊绕过 |

详见 `reference/README.md`。

---

## 常见问题

**Q: 编译失败怎么办？**

确保 Rust 版本 >= 1.75：

```bash
rustup update stable
```

**Q: 提示 "Unsupported version: 1.xx"**

当前支持的版本列表见 `src/protocol/mod.rs`。用 `-v 1.21.1` 试试。

**Q: 代理用不了？**

确保代理 URL 格式正确：`socks5://host:port`。`proxy.verify` 默认开启，会验证代理是否可用。

**Q: 怎么退出程序？**

按 `Ctrl+C`。

**Q: 服务装好了怎么改配置？**

编辑 `~/.config/creeper/config.json`，然后重启服务：

```bash
creeper service restart     # 暂不支持，请先 stop 再 start
```
