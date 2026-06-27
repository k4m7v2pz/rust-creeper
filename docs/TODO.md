# LambdaAttack — 项目状态

## ✅ 已完成

### 基础设施

- [x] Rust Tokio 项目结构（单 crate，`src/` 拍平）
- [x] CLI 入口（clap 子命令：`start` / `config` / `service` / `info`）
- [x] JSON 配置管理（`~/.config/lambdaattack/config.json`）
- [x] JSON Schema + 默认/示例配置（`config/`）
- [x] 文档（`docs/design.md` 架构说明 + `docs/usage.md` 使用指南）
- [x] 跨平台系统服务（daemon-kit: systemd / launchd / Windows Service）
- [x] `.gitignore` 全面覆盖

### 协议层

- [x] `GameVersion` 枚举 — 26 个版本（1.11 ~ 1.21.7）
- [x] `UniversalProtocol` / `BotSession` / `SessionListener` trait 定义
- [x] 6 个版本占位 crate（1.11 / 1.12.2 / 1.14.4 / 1.15.2 / 1.16.5 / 1.21+）
- [x] `factory.rs` 所有版本映射

### 核心逻辑

- [x] `LambdaAttack` 编排器（Bot 创建 + 延迟加入）
- [x] `Bot` + `BotHandle`（tokio::spawn + mpsc channel）
- [x] 重试机制（`max_attempts`，被踢自动重连）
- [x] SOCKS4 / SOCKS5 / HTTP 代理支持（`proxied` crate）
- [x] 代理文件加载（兼容 URL 格式 + 裸 host:port）
- [x] 昵称/代理列表文件加载

### 参考项目（已缝合在 `reference/`）

- [x] GeyserMC/MCProtocolLib — Java MC 协议库（MIT）
- [x] crpmax/mc-bots — Java MC 机器人（MIT）
- [x] Minecraft-Holy-Client — C# 压力测试（Apache 2.0）
- [x] EndMinecraftPlusV2 — Java，支持 Forge + 反作弊绕过
- [x] MinecraftMotdStressTest — Python MOTD 压测（MIT）

---

## ❌ 未完成 / 待实现

### 协议实现（占位需填实）

- [ ] **TCP 连接** — `session.connect()` 目前只打日志，未真正对接 MC 协议
- [ ] **客户端握手** — LoginStart → Encryption → LoginPlugin → JoinGame
- [ ] **ServerListPing** — `lambdaattack info` 命令还是空的，参考 `reference/mc-bots-ref/ServerInfo.java`
- [ ] **数据包交互** — 至少实现位置同步、聊天收发、踢出处理
- [ ] **KeepAlive** — 保活包自动回复

### Forge 支持

- [ ] **Forge V1 握手**（1.7 ~ 1.12.2）— 参考 `reference/endmc-ref/MCForgeHandShakeV1.java`
- [ ] **Forge V2 握手**（1.13+）— 参考 `reference/endmc-ref/MCForgeHandShakeV2.java`
- [ ] **Mod 列表注入** — host 追加 `\0FML\0` / `\0FML2\0`
- [ ] **反作弊绕过** — 参考 `reference/endmc-ref/ACProtocol/`

### 代理

- [ ] **代理验证** — 通过代理向目标服务器发起 TCP 连接 + ServerListPing 测试可用性
- [ ] **代理轮换** — bot 被踢后自动换代理重连
- [ ] **代理自动抓取** — 支持从 URL 下载代理列表

### 功能增强

- [ ] **TUI**（ratatui）— 实时仪表盘：bot 数量、延迟、在线状态、日志
- [ ] **REST API**（axum）— WebUI 控制端
- [ ] **昵称生成器** — 参考 `reference/mc-bots-ref/NickGenerator.java`
- [ ] **自动重生** — `auto_respawn_delay_ms` 已配但未实现
- [ ] **加入命令** — `join_commands` 已配但未实现
- [ ] **服务器信息探测** — MOTD 渲染、玩家数、版本、延迟

### 工程化

- [ ] **CI/CD** — GitHub Actions 自动编译 + 发布二进制
- [ ] **二进制发布** — GitHub Releases
- [ ] **`--version` 显示编译时间/commit**（`built` 或 `vergen` crate）

---

*最后更新：2026-06-27*
