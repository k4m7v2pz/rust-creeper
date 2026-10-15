# Creeper — 项目状态

## ✅ 已完成

### 基础设施

- [x] Rust Tokio 项目结构（单 crate，`src/` 拍平）
- [x] CLI 入口（clap 子命令：`start` / `config` / `service` / `info`）
- [x] JSON 配置管理（`~/.config/creeper/config.json`）
- [x] JSON Schema + 默认/示例配置（`config/`）
- [x] 文档（`docs/design.md` 架构说明 + `docs/usage.md` 使用指南）
- [x] 跨平台系统服务（daemon-kit: systemd / launchd / Windows Service）
- [x] `.gitignore` 全面覆盖
- [x] 敏感信息守卫（`hooks/pre-commit` + `scripts/bash/check-sensitive.sh`）

### 协议层

- [x] `GameVersion` 枚举 — 26 个版本（1.11 ~ 1.21.7）
- [x] `UniversalProtocol` / `BotSession` / `SessionListener` trait 定义
- [x] 6 个版本占位 crate（1.11 / 1.12.2 / 1.14.4 / 1.15.2 / 1.16.5 / 1.21+）
- [x] `factory.rs` 所有版本映射

### 核心逻辑

- [x] `Creeper` 编排器（Bot 创建 + 延迟加入）
- [x] `Bot` + `BotHandle`（tokio::spawn + mpsc channel）
- [x] 重试机制（`max_attempts`，被踢自动重连）
- [x] 代理列表加载（文件 / URL / 内联，兼容 URL 格式 + 裸 host:port）
- [x] 昵称/代理列表文件加载
- [x] 自动注册（`--register` / `-r`：加入后发 `/register` 和 `/login`）

### 真协议会话（1.17+，`src/real_session.rs`）

- [x] TCP 连接 + Handshake（state=login）
- [x] LoginStart → LoginSuccess / Login Disconnect
- [x] KeepAlive 自动回复（后台 reader 线程）
- [x] 聊天收发（system chat 解析 + `send_chat`）
- [x] 踢出 / 断线处理（`on_disconnected` 回调）
- [x] 主动断开（PLAY 态 Disconnect 包）

### 参考项目（已缝合在 `reference/`）

- [x] GeyserMC/MCProtocolLib — Java MC 协议库（MIT）
- [x] crpmax/mc-bots — Java MC 机器人（MIT）
- [x] Minecraft-Holy-Client — C# 压力测试（Apache 2.0）
- [x] EndMinecraftPlusV2 — Java，支持 Forge + 反作弊绕过
- [x] MinecraftMotdStressTest — Python MOTD 压测（MIT）

### 功能增强

- [x] **TUI**（ratatui）— 实时仪表盘：bot 数量、延迟、在线状态、日志（`creeper tui`）
- [x] **REST API**（axum）— Hub 服务端已实现（`creeper hub-serve`）
- [x] **服务器信息探测** — MOTD 渲染、玩家数、版本、延迟（`creeper mc-info`，`src/motd.rs` 的 `ping_java` / `ping_bedrock`）
- [x] **昵称生成器** — 参考 `reference/mc-bots-ref/NickGenerator.java`
- [x] **MCP 接入** — stdio + HTTP 双 transport，6 只读工具，审计日志（`creeper mcp`）
- [x] **安装教程** — `docs/INSTALL.md`（cargo install 常驻副本 + MCP 接入）

---

## ❌ 未完成 / 待实现

### 协议实现（1.17+ 会话缺口）

- [ ] **加密握手（Encryption）** — `real_session.rs` 未处理 0x01 Encryption Request，online-mode 服务器连不上
- [ ] **Login Plugin 响应** — 0x03 LoginPluginRequest 目前只打日志跳过、不响应（Forge V2 需要）
- [ ] **JoinGame / 进入游戏** — 登录成功直接进 PLAY，不读 JoinGame 包，无位置同步
- [ ] **PLAY 态数据包扩展** — 位置同步、实体、Inventory 等（聊天/踢出/KeepAlive 已实现）

### 旧版本协议（1.11 ~ 1.16.5）

- [ ] **真连接填实** — 占位 crate 的 connect 走 real_session，但 `packet_ids()` 对旧版返回 None → 全部回落 StubSession（假连接，只打日志）
- [ ] **各版本数据包映射** — packet ID 表仅覆盖 1.17.1 ~ 1.21.4

### Forge 支持

- [ ] **Forge V1 握手**（1.7 ~ 1.12.2）— 参考 `reference/endmc-ref/MCForgeHandShakeV1.java`
- [ ] **Forge V2 握手**（1.13+）— 参考 `reference/endmc-ref/MCForgeHandShakeV2.java`
- [ ] **Mod 列表注入** — host 追加 `\0FML\0` / `\0FML2\0`
- [ ] **反作弊绕过** — 参考 `reference/endmc-ref/ACProtocol/`

### 代理

- [ ] **MC 机器人代理接入** — `bot_connect.rs` / `real_session.rs` 连接路径未使用代理（`_proxy` 参数被忽略）；代理列表已能加载但从未消费
- [ ] **代理验证** — `config.proxy.verify` 字段存在但无调用；design 声称的「TCP + ServerListPing 验证」未实现
- [ ] **代理轮换** — bot 被踢后换代理重连（当前重连机制存在但固定原连接）
- [ ] **代理自动抓取** — `config.proxy.url` 字段存在，无下载逻辑

### 功能增强

- [ ] **自动重生** — `auto_respawn_delay_ms` 已配置但未实现
- [ ] **加入命令** — `join_commands` 已配置但未实现

### 工程化

- [ ] **CI/CD** — GitHub Actions 自动编译 + 发布二进制（当前无 `.github/`）
- [ ] **二进制发布** — GitHub Releases
- [ ] **`--version` 显示编译时间/commit**（`built` 或 `vergen` crate）

---

*最后更新：2026-10-16（对照代码核查：`src/real_session.rs` / `src/motd.rs` / `src/bot.rs` / `src/config.rs`）*
