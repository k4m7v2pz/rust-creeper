# LambdaAttack 设计文档

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

- 文件：`~/.config/lambdaattack/config.json`
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

安装后自动随系统启动，`lambdaattack service run` 为前台模式供服务管理器调用。

## 反机器人绕过（待实现）

参考 `reference/endmc-ref/AdvanceModule/ACProtocol/`：

- **重试机制**: 配置 `max_attempts`，被踢后换代理重连
- **AntiCheat3**: 自定义数据包/时间戳绕过
- **CatAntiCheat**: 猫反作弊协议处理
- **AnotherStarAntiCheat**: AnotherStar 反作弊协议处理
