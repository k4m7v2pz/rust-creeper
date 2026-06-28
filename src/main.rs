use std::path::PathBuf;

use clap::{Parser, Subcommand};
use config::Config;
use crate::attack::LambdaAttack;

mod attack;
mod bot;
mod bot_connect;
mod config;
mod entity_location;
mod factory;
mod http_flood;
mod logging;
mod motd;
mod options;
mod protocol;
mod service;

#[derive(Parser)]
#[command(name = "lambdaattack", version, about = "Minecraft stress-test bot")]
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

    /// Monitor server — periodically ping and collect player names
    #[command(visible_alias = "m")]
    Monitor {
        /// Server hostname
        host: Option<String>,

        /// Server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// Ping interval in seconds
        #[arg(short = 't', long, default_value_t = 60)]
        interval: u64,
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
    #[command(visible_alias = "c")]
    Check {
        /// Domain name to check
        domain: String,
    },

    /// Manage system service (install/start/stop/status)
    #[command(visible_alias = "sv")]
    Service {
        #[command(subcommand)]
        action: ServiceAction,
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

        Some(Commands::Monitor { host, port, interval }) => {
            let host = host.unwrap_or_else(|| "127.0.0.1".to_string());
            let interval = std::time::Duration::from_secs(interval);

            log::info!("Starting monitor for {}:{} (every {}s)", host, port, interval.as_secs());

            // First query — get version info and set protocol_version
            let result = motd::ping(&host, port, -1, false);
            if result.info.motd.is_empty() {
                log::error!("Server {}:{} is unreachable, aborting monitor", host, port);
                eprintln!("Server is offline or unreachable.");
                return Ok(());
            }

            motd::print_server_info(&result);
            println!();
            log::info!("Monitoring — press Ctrl+C to stop and save journal");

            motd::monitor(&host, port, -1, interval).await?;
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

        Some(Commands::Check { domain }) => {
            let check = http_flood::check_cdn(&domain);
            http_flood::print_cdn_check(&check);
        }

        Some(Commands::Service { action }) => {
            use service::daemon;
            let d = daemon();
            match action {
                ServiceAction::Install => {
                    d.install_service()?;
                    log::info!("lambdaattack service installed");
                }
                ServiceAction::Uninstall => {
                    d.uninstall_service()?;
                    log::info!("lambdaattack service uninstalled");
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

    log::info!("Starting LambdaAttack → {}:{} ({} bots, max_attempts={})",
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

    let mut attack = LambdaAttack::new();
    if !proxies.is_empty() { attack.set_proxies(proxies); }
    if !nicks.is_empty() { attack.set_names(nicks); }
    attack.start(options).await;
}
