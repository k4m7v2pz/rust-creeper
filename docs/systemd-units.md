# Creeper systemd 服务部署示例

Creeper 的长期任务（监控 / 爬取 / 上报）通过 systemd 管理，共 6 个单元文件：
hub、monitor、crawl、worker 四个服务 + backup 服务/定时器。

> **脱敏说明**：本文档为公开示例。部署路径 `<DEPLOY_DIR>`（如 `/opt/creeper`）、
> 文档链接 `<ORG>`、hub 地址 `<HUB_URL>` 均需按实际环境替换。
> 单元文件本身不含敏感数据；**敏感数据位于数据文件**（如 `mc-targets.json` 中的
> 真实服务器地址、`player-journal.json` 中的玩家名），这些文件**禁止提交到仓库**。

## 服务总览

| 单元文件 | 类型 | 角色 |
|----------|------|------|
| `creeper-hub.service` | simple | MC 玩家名册同步 API（监听 `:9090`） |
| `creeper-monitor.service` | simple | 多目标 MC 服务器循环探测，sync 到 hub |
| `creeper-crawl.service` | simple | 慢速长跑扫描 pending 目标（低并发） |
| `creeper-worker.service` | simple | 分布式扫描任务执行器（从 hub poll 任务） |
| `creeper-backup.service` | oneshot | 每日数据打包 .zip |
| `creeper-backup.timer` | timer | 每日 03:00 触发 backup |

## 1. Hub — 玩家名册同步 API

`/etc/systemd/system/creeper-hub.service`

```ini
[Unit]
Description=Creeper Hub — MC player journal sync API
Documentation=https://github.com/<ORG>/creeper
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=<RUN_USER>
Group=<RUN_USER>
Environment=CREEPER_DATA_DIR=<DEPLOY_DIR>/data
ExecStart=<DEPLOY_DIR>/creeper hub-serve --host 0.0.0.0 --port 9090 --target mc-hub
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=<DEPLOY_DIR>/data
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

> `--host 0.0.0.0` 会暴露到所有网卡。如需仅本机访问，改为 `127.0.0.1`。

## 2. Monitor — 多目标 MC 服务器循环探测

`/etc/systemd/system/creeper-monitor.service`

```ini
[Unit]
Description=Creeper Monitor — Multi-target MC server reconnaissance
Documentation=https://github.com/<ORG>/creeper
After=network-online.target creeper-hub.service
Wants=network-online.target creeper-hub.service

[Service]
Type=simple
User=<RUN_USER>
Group=<RUN_USER>
Environment=CREEPER_DATA_DIR=<DEPLOY_DIR>/data
WorkingDirectory=<DEPLOY_DIR>
ExecStart=<DEPLOY_DIR>/creeper mc-monitor --targets <DEPLOY_DIR>/data/mc-targets.json --hub <HUB_URL>
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=<DEPLOY_DIR>/data
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

`mc-targets.json` 结构（示例，地址已脱敏）：

```json
{
  "targets": [
    {
      "provider": "<示例服>",
      "host": "example.example.com",
      "port": 25565,
      "interval_secs": 60,
      "version": "1.21.4",
      "notes": "可选备注"
    }
  ]
}
```

## 3. Crawl — 慢速长跑扫描

`/etc/systemd/system/creeper-crawl.service`

```ini
[Unit]
Description=Creeper MC Server Crawler (slow exploration)
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=<RUN_USER>
WorkingDirectory=<DEPLOY_DIR>
Environment=RUST_LOG=info
ExecStart=<DEPLOY_DIR>/creeper mc-crawl --targets <DEPLOY_DIR>/data/mc-targets.json --round-delay 600 --port-delay-ms 3000
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

> **注意**：子命令是 `mc-crawl`（别名 `mcc`），不是 `crawl`。
> 二进制重建后子命令改名时，旧的单元文件若仍写 `crawl` 会导致启动即报
> `unrecognized subcommand 'crawl'` 并无限重启。改完单元文件后需
> `systemctl daemon-reload` 再 `restart`。

## 4. Worker — 分布式任务执行器

`/etc/systemd/system/creeper-worker.service`

```ini
[Unit]
Description=Creeper Task Worker — distributed scan task executor
After=network-online.target creeper-hub.service
Wants=network-online.target creeper-hub.service

[Service]
Type=simple
User=<RUN_USER>
Group=<RUN_USER>
WorkingDirectory=<DEPLOY_DIR>
Environment=RUST_LOG=info
ExecStart=<DEPLOY_DIR>/creeper worker --hub <HUB_URL> --poll-interval 60
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=<DEPLOY_DIR>/data
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

## 5. Backup — 每日数据备份

`/etc/systemd/system/creeper-backup.service`

```ini
[Unit]
Description=Creeper — daily data backup (.zip)
Documentation=https://github.com/<ORG>/creeper

[Service]
Type=oneshot
User=<RUN_USER>
Group=<RUN_USER>
Environment=CREEPER_DATA_DIR=<DEPLOY_DIR>/data
ExecStart=<DEPLOY_DIR>/creeper backup --output <DEPLOY_DIR>/data/backups
StandardOutput=journal
StandardError=journal

# Security hardening — same as creeper-hub.service
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=<DEPLOY_DIR>/data
PrivateTmp=yes
```

`/etc/systemd/system/creeper-backup.timer`

```ini
[Unit]
Description=Creeper — daily data backup (runs at 03:00)
Documentation=https://github.com/<ORG>/creeper

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

> `OnCalendar=daily` 默认凌晨 03:00 触发；如需指定时刻，如 `OnCalendar=*-*-* 04:30:00`。

## 部署步骤

```bash
# 1. 将单元文件复制到 systemd 目录（按实际替换 <RUN_USER> / <DEPLOY_DIR> / <HUB_URL>）
sudo cp creeper-*.service creeper-backup.timer /etc/systemd/system/

# 2. 重新加载并启用
sudo systemctl daemon-reload
sudo systemctl enable --now creeper-hub.service creeper-monitor.service \
  creeper-crawl.service creeper-worker.service creeper-backup.timer

# 3. 查看状态与日志
systemctl status creeper-hub.service
journalctl -u creeper-monitor.service -f
```

## 常见故障排查

- **服务反复重启（activating auto-restart）**：`journalctl -u <单元> -e` 看退出原因，
  常见为子命令不存在（见上文 crawl 改名坑）、路径/权限错误。
- **monitor 加载不到新目标**：编辑 `mc-targets.json` 后需重启 monitor
  （当前实现启动时读取一次，不热加载）。
- **hub 无法访问**：确认 `creeper-hub.service` 先于 monitor/worker 启动
  （单元中已通过 `After=` / `Wants=` 声明依赖）。
