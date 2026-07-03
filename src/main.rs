use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use config::Config;
use crate::attack::Creeper;

mod attack;
mod bot;
mod bot_connect;
mod c2core;
mod config;
mod crawl;
mod discover;
mod tasks;
mod entity_location;
mod factory;
mod http_flood;
mod logging;
mod motd;
mod options;
mod protocol;
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
    /// Start attack with current config / CLI overrides
    #[command(visible_alias = "s")]
    Start {
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

    /// Print server info (MOTD, players, version)
    #[command(visible_alias = "i")]
    Info {
        /// Server hostname
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,
    },

    /// Monitor server(s) — periodically ping and collect player names.
    /// Use --targets to monitor multiple servers from a JSON file.
    #[command(visible_alias = "m")]
    Monitor {
        /// Server hostname (single target mode)
        host: Option<String>,

        /// Server port (single target mode)
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Ping interval in seconds (single target mode)
        #[arg(short = 't', long, default_value_t = 60)]
        interval: u64,

        /// Hub URL to sync journal to (e.g. http://printer:9090).
        /// Falls back to `hub.url` in config, then http://127.0.0.1:9090.
        #[arg(long)]
        hub: Option<String>,

        /// Path to targets JSON file for multi-target monitoring
        #[arg(long)]
        targets: Option<String>,
    },

    /// Discover MC servers by scanning domain patterns + port ranges.
    /// Example: creeper discover --domains "srv{}.example.com:1..50" --ports "25565,10000-10500"
    #[command(visible_alias = "d")]
    Discover {
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

        /// Output as JSON array (for mc-targets.json)
        #[arg(long)]
        json: bool,
    },

    /// Send HTTP/HTTPS flood (concurrent requests)
    #[command(visible_alias = "h")]
    Http {
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

    /// Check if a domain is behind a CDN (Cloudflare, etc.)
    #[command(visible_alias = "ck")]
    Check {
        /// Domain name to check
        domain: String,
    },

    /// Query a Creeper Hub node for status and player journal.
    #[command(visible_alias = "hb")]
    Hub {
        /// Hub URL (e.g. http://localhost:9090). Optional: falls back to `hub.url` in config,
        /// then http://127.0.0.1:9090. Auto-prepends `http://` if no scheme.
        url: Option<String>,

        /// Override the hub URL (same as the positional argument).
        #[arg(long)]
        hub: Option<String>,
    },

    /// Submit a scan task to the Hub for distributed processing.
    #[command(visible_alias = "sub")]
    Submit {
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

    /// Worker mode: pull tasks from Hub and execute them.
    #[command(visible_alias = "wk")]
    Worker {
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

    /// List tasks from Hub.
    #[command(visible_alias = "tl")]
    TaskList {
        /// Hub URL (e.g. http://printer:9090)
        #[arg(long)]
        hub: Option<String>,
    },

    /// Terminal UI dashboard (monitor + flood control)
    #[command(visible_alias = "t")]
    Tui {
        /// Server hostname to monitor
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Hub URL to pull journal from (e.g. http://printer:9090).
        /// Falls back to `hub.url` in config, then http://127.0.0.1:9090.
        #[arg(long)]
        hub: Option<String>,
    },

    /// Manage system service (install/start/stop/status)
    #[command(visible_alias = "sv")]
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Start sync hub — serve journal API for other nodes
    #[command(visible_alias = "srv")]
    Serve {
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

    /// Show player journal (local or from a hub)
    #[command(visible_alias = "j")]
    Journal {
        /// Server hostname filter
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Hub URL to pull from (e.g. http://printer:9090).
        /// Falls back to `hub.url` in config, then http://127.0.0.1:9090.
        #[arg(long)]
        hub: Option<String>,
    },

    /// Show hub status. With --scope, fetch richer data from the hub.
    ///   `creeper status`                  → node meta only (health, uptime, target, total players)
    ///   `creeper status --scope players`  → full player journal (per-server player names + roles)
    ///   `creeper status --scope servers`  → per-server overview (server → player count + names)
    ///   `creeper status --scope all`      → merged JSON: node meta + servers + players
    Status {
        /// Hub URL (e.g. http://printer:9090). Optional: falls back to `hub.url` in config,
        /// then http://127.0.0.1:9090. Auto-prepends `http://` if no scheme.
        url: Option<String>,

        /// What to fetch: `players`, `servers`, `all`. Omit for node meta only.
        #[arg(short = 's', long)]
        scope: Option<StatusScope>,

        /// Override the hub URL (same as the positional `url`).
        #[arg(long)]
        hub: Option<String>,
    },

    /// Annotate a player (set role/note)
    #[command(visible_alias = "a")]
    Annotate {
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

    /// Build a portable C2 agent binary (educational use)
    #[command(visible_alias = "b")]
    Build {
        /// C2 server address(es) to hardcode into the agent
        #[arg(short = 'S', long = "server", required = true)]
        servers: Vec<String>,
    },

    /// Crawl pending_scan domains slowly over time (long-running exploration)
    /// Designed for Arch node — low concurrency, long delays between rounds
    #[command(visible_alias = "cr")]
    Crawl {
        /// Path to mc-targets.json (default: ./data/mc-targets.json)
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

    /// Scan IPv4 range + ports with protocol detection (nmap-like)
    #[command(visible_alias = "sc")]
    Scan {
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

    /// Probe local system info — OS, admin, available shells
    Probe,
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
    #[command(visible_alias = "sh")]
    SetHub {
        /// Hub URL, e.g. `http://<your-hub-ip>:9090` or just `<your-hub-ip>:9090`
        /// (http:// is auto-prepended if no scheme).
        url: String,
    },
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
                        // Load existing config (or default), update hub.url, save
                        let mut cfg = if path.exists() {
                            Config::load_from(Some(&path)).unwrap_or_default()
                        } else {
                            Config::default()
                        };
                        cfg.hub.url = normalized.clone();
                        cfg.save_to(&path)?;
                        println!("✓ Hub URL set to {}", normalized);
                        println!("  Saved to {}", path.display());
                        println!();
                        println!("  Now `creeper status`, `creeper hub`, `creeper journal`, `creeper tui`");
                        println!("  will automatically use this hub. No need to pass --hub each time.");
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

        Some(Commands::Start { host, port, amount, delay, name_format, version,
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

        Some(Commands::Discover { domains, ports, concurrency, json }) => {
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

        Some(Commands::Info { host, port }) => {
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

        Some(Commands::Monitor { host, port, interval, hub: hub_cli, targets }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub_cli.as_deref());
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

        Some(Commands::Http { url, method, concurrency, total, body, delay, header, proxy, timeout }) => {
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

        Some(Commands::Hub { url, hub }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(url.as_deref().or(hub.as_deref()));
            sync::print_node_status(&hub_url).await;
        }

        Some(Commands::Submit { hub, domains, ports, concurrency }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub.as_deref());

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

        Some(Commands::Worker { hub, worker_id, poll_interval }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub.as_deref());
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

                                log::info!("[Worker] Claimed task: {} ({} domains, {} ports)",
                                    task_id, domains.len(), ports.len());

                                let patterns: Vec<discover::DomainPattern> = domains
                                    .iter()
                                    .map(|d| discover::DomainPattern::parse(d))
                                    .collect::<Result<_, _>>()?;

                                let servers = discover::discover(&patterns, &ports, 5).await;

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

        Some(Commands::TaskList { hub }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub.as_deref());

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

        Some(Commands::Check { domain }) => {
            let check = http_flood::check_cdn(&domain);
            http_flood::print_cdn_check(&check);
        }

        Some(Commands::Build { servers }) => {
            use std::net::ToSocketAddrs;
            println!("Building C2 agent for servers (ordered by priority):");
            for (i, addr) in servers.iter().enumerate() {
                let label = if i == 0 { "Primary" } else { "Fallback" };
                match addr.to_socket_addrs() {
                    Ok(_) => println!("  ✔ {}: {}", label, addr),
                    Err(e) => {
                        log::error!("Invalid server address '{}': {}", addr, e);
                        eprintln!("  ✘ {}: {} (unresolvable)", label, addr);
                        return Err(anyhow::anyhow!("Cannot resolve '{}'", addr));
                    }
                }
            }
            if servers.len() == 1 {
                println!("  └─ (no fallback servers)");
            }
            // TODO: actual payload generation
            log::warn!("Payload generation not yet implemented — this is a placeholder");
        }

        Some(Commands::Crawl { targets, ports, concurrency, round_delay, port_delay_ms }) => {
            let targets_path = targets.unwrap_or_else(|| "./data/mc-targets.json".to_string());
            
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

        Some(Commands::Scan { target, ports, modes, concurrency }) => {
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

        Some(Commands::Probe) => {
            let info = c2core::collect_info();
            println!("┌─ System Probe ────────────────────────────");
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

        Some(Commands::Serve { host, port, target }) => {
            sync::run_hub(&host, port, &target).await?;
        }

        Some(Commands::Journal { host, port, hub: hub_cli }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub_cli.as_deref());

            if hub_cli.is_some() || !cfg.hub.url.is_empty() {
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

        Some(Commands::Status { url, hub, scope }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(url.as_deref().or(hub.as_deref()));

            match scope {
                // `creeper status` (no scope) → node meta only (back-compat)
                None => {
                    if let Some(status) = sync::pull_status(&hub_url).await {
                        println!("{}", serde_json::to_string_pretty(&status).unwrap_or_default());
                    } else {
                        eprintln!("Failed to pull status from {}", hub_url);
                    }
                }
                // `creeper status players` → full player journal
                Some(StatusScope::Players) => {
                    if let Some(j) = sync::pull_from_hub(&hub_url).await {
                        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
                    } else {
                        eprintln!("Failed to pull journal from {}", hub_url);
                    }
                }
                // `creeper status servers` → per-server overview
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
                // `creeper status all` → merged: node meta + servers + players
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
                    merged.insert("hub".into(), serde_json::json!(hub_url));
                    println!("{}", serde_json::to_string_pretty(&merged).unwrap_or_default());
                }
            }
        }

        Some(Commands::Annotate { host, port, player, role, note }) => {
            let host = host.unwrap_or_else(|| "?".into());
            let mut journal = motd::PlayerJournal::load();
            journal.annotate(&host, port, &player, &role, &note);
            journal.save();
            journal.print_summary(&host, port);
        }

        Some(Commands::Tui { host: _, port: _, hub: hub_cli }) => {
            let cfg = Config::load();
            let hub_url = cfg.resolve_hub_url(hub_cli.as_deref());
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
