# Changelog

## 2026-07

### 攻击链模块 + HIDS 自检

- `feat`: 实现攻击链 wipe/harvest/persist 三个模块
- `feat`: HIDS 主机入侵指标自检（`hids-check`）
- `feat`: backup 命令，支持本地和远程 Hub 拉取备份
- `feat`: 数据持久化修复

### CLI 命名重构

- CLI 子命令统一为 `mc-*` / `hub-*` / `host-*` 命名空间
- `start` → `mc-stress`，`info` → `mc-info`，`monitor` → `mc-monitor`
- 新增 `host-monitor` 本机探针命令（仅监控 CPU/RAM/磁盘，不上报 MC 数据）
- 新增 `host-info` 本机系统信息命令

### Hub 分布式系统

- Hub 多节点多连接方式 schema（IPv4/IPv6/内网穿透/WS 反向连接）
- 分布式任务队列系统（`hub-submit` / `hub-worker` / `hub-tasks`）
- `hub-query` 多 scope 查询（players/servers/all/hosts）
- Hub URL 配置化（`hub.default_node`），CLI 可覆盖
- 自动补全 `http://` 前缀

### 发现与扫描

- `mc-discover`：域名模板 + 端口段扫描
- `mc-crawl`：慢速长跑扫描（低并发、长间隔，适合潜伏探索）
- `port-scan`：IPv4 CIDR/范围 + 端口段，多协议探测（TCP/UDP/HTTP/ICMP）

### 其他

- `cdn-check`：检测域名是否在 CDN 后面
- `http-flood`：HTTP/HTTPS 洪水攻击，支持所有方法 + 代理轮换
- `tui`：ratatui 终端仪表盘
- 跨平台系统服务（daemon-kit: systemd / launchd / Windows Service）
- MOTD 协议探测集成
- 玩家日志（PlayerJournal）持久化，SRV 解析
- AGENTS.md — Agent 协作规约，数据脱敏政策

---

## 2026-06

### 初始 Rust 移植

- Rust Tokio 端口 of LambdaAttack（Java → Rust）
- CLI 入口（clap 子命令），JSON 配置管理
- 代理支持（SOCKS4/5, HTTP CONNECT）
- Minecraft 协议版本支持（1.11 ~ 1.21.7）
- 参考项目缝合至 `reference/` 目录

---

*历史版本 2.0 ~ 2.3 见原始 LambdaAttack 项目，与本项目无关。*