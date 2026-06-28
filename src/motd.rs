//! MOTD / server-info query module.
//!
//! Wraps [`gamedig`] for querying Minecraft Java Edition (TCP) and
//! Bedrock Edition (UDP/RakNet) servers.  Query results can be cached
//! as JSON files for offline analysis and program improvement.

use std::fs;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::Local;
use gamedig::games::minecraft::{self, RequestSettings};
use gamedig::protocols::types::CommonResponse;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Server status information from a successful ping (edition-agnostic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Minecraft version string (e.g. "1.21.1" or "1.21.11")
    pub game_version: String,
    /// Protocol version number (Java) or `-1` for Bedrock
    pub protocol_version: i32,
    /// Edition label: "Java" or "Bedrock"
    pub edition: String,
    /// Server MOTD / description text (plain, stripped of formatting codes)
    pub motd: String,
    /// Online player count
    pub players_online: u32,
    /// Maximum player count
    pub players_max: u32,
    /// Online player names collected from SLP sample (may be empty/incomplete)
    pub players: Vec<String>,
    /// Base64-encoded favicon PNG (Java only, optional)
    pub favicon: Option<String>,
}

/// Combined ping result indicating which protocol succeeded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub info: ServerInfo,
    pub protocol: Protocol,
    /// Cache file path (if caching is enabled and this was not a cache hit)
    pub cached_at: Option<String>,
}

/// Which Minecraft protocol was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Java,
    Bedrock,
}

/// Cached query entry on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// UNIX timestamp of when the query was made
    timestamp: u64,
    /// Human-readable timestamp
    timestamp_human: String,
    /// Which protocol responded
    protocol: Protocol,
    /// The full server info
    info: ServerInfo,
    /// The raw description JSON from the server (for post-mortem analysis)
    raw_description: Option<String>,
    /// Raw favicon base64 (Java)
    raw_favicon: Option<String>,
}

// ---------------------------------------------------------------------------
// Cache helpers
// ---------------------------------------------------------------------------

/// Return the cache directory, creating it if needed.
fn cache_dir() -> Result<PathBuf> {
    let base = directories::ProjectDirs::from("", "", "lambdaattack")
        .context("Cannot determine cache directory")?
        .cache_dir()
        .join("motd-cache");
    fs::create_dir_all(&base).context("Failed to create MOTD cache directory")?;
    Ok(base)
}

/// File-name-safe key for a host:port pair.
fn cache_key(host: &str, port: u16) -> String {
    let safe_host = host.replace('.', "_").replace(':', "_");
    format!("{}_{}.json", safe_host, port)
}

/// Try to load a cached query result.  Returns `None` if the cache is absent,
/// stale (> 1 hour), or unreadable.
fn cache_load(host: &str, port: u16) -> Option<PingResult> {
    let path = cache_dir().ok()?.join(cache_key(host, port));
    if !path.exists() {
        return None;
    }
    let data = fs::read_to_string(&path).ok()?;
    let entry: CacheEntry = serde_json::from_str(&data).ok()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Cache valid for 1 hour
    if now.saturating_sub(entry.timestamp) > 3600 {
        let _ = fs::remove_file(&path); // stale, clean up
        return None;
    }

    Some(PingResult {
        info: entry.info,
        protocol: entry.protocol,
        cached_at: Some(path.to_string_lossy().to_string()),
    })
}

/// Save a query result to the cache.
fn cache_save(host: &str, port: u16, protocol: Protocol, info: &ServerInfo, raw_desc: Option<String>, raw_icon: Option<String>) {
    let path = match cache_dir() {
        Ok(d) => d.join(cache_key(host, port)),
        Err(_) => return,
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let entry = CacheEntry {
        timestamp: now,
        timestamp_human: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        protocol,
        info: info.clone(),
        raw_description: raw_desc,
        raw_favicon: raw_icon,
    };

    if let Ok(json) = serde_json::to_string_pretty(&entry) {
        let _ = fs::write(&path, json);
    }
}

// ---------------------------------------------------------------------------
// Edition-specific query functions
// ---------------------------------------------------------------------------

/// Resolve hostname to IP address.
fn resolve(host: &str, port: u16) -> Result<IpAddr> {
    Ok(format!("{}:{}", host, port)
        .to_socket_addrs()
        .context("DNS resolution failed")?
        .next()
        .context("No addresses resolved")?
        .ip())
}

/// Ping a Java Edition (TCP) server.
///
/// `protocol_version` — use `-1` for "any version", or a specific number
/// (e.g. 767 for 1.21(.1), 766 for 1.20.6, etc.).
pub fn ping_java(host: &str, port: u16, protocol_version: i32) -> Result<ServerInfo> {
    let ip = resolve(host, port)?;
    let settings = RequestSettings { hostname: host.to_owned(), protocol_version };
    let resp = minecraft::query_java(&ip, Some(port), Some(settings))
        .map_err(|e| anyhow::anyhow!("Java ping failed: {e}"))?;

    Ok(ServerInfo {
        edition: "Java".into(),
        game_version: resp.game_version.clone(),
        protocol_version: resp.protocol_version,
        motd: strip_motd_formatting(&resp.description),
        players_online: resp.players_online,
        players_max: resp.players_maximum,
        players: resp.players
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.name)
            .collect(),
        favicon: resp.favicon.clone(),
    })
}

/// Ping a Bedrock Edition (UDP / RakNet) server.
pub fn ping_bedrock(host: &str, port: u16) -> Result<ServerInfo> {
    let ip = resolve(host, port)?;
    let resp = minecraft::query_bedrock(&ip, Some(port))
        .map_err(|e| anyhow::anyhow!("Bedrock ping failed: {e}"))?;

    // BedrockResponse implements CommonResponse, so we use the trait methods.
    // These return Option<&str>; unwrap_or provides a fallback.
    let desc = resp.description().unwrap_or("");
    let version = resp.game_version().unwrap_or("unknown");
    let online = resp.players_online();
    let max = resp.players_maximum();

    Ok(ServerInfo {
        edition: "Bedrock".into(),
        game_version: version.to_string(),
        protocol_version: -1, // Bedrock doesn't expose a protocol version via gamedig
        motd: strip_motd_formatting(desc),
        players_online: online,
        players_max: max,
        players: vec![],
        favicon: None, // Bedrock doesn't have a favicon
    })
}

/// Try Java first, then Bedrock.  Returns a `PingResult` with the protocol
/// that responded, plus a `cached_at` field if the result came from cache.
///
/// When `use_cache` is `true` (default), a fresh query will be saved to cache
/// and a subsequent call within 1 hour will return the cached result.
pub fn ping(host: &str, port: u16, protocol_version: i32, use_cache: bool) -> PingResult {
    // Try cache first
    if use_cache {
        if let Some(cached) = cache_load(host, port) {
            return cached;
        }
    }

    // Try Java
    match ping_java(host, port, protocol_version) {
        Ok(info) => {
            let result = PingResult {
                info: info.clone(),
                protocol: Protocol::Java,
                cached_at: None,
            };
            if use_cache {
                cache_save(host, port, Protocol::Java, &info, None, None);
            }
            return result;
        }
        Err(java_err) => {
            log::debug!("Java ping failed for {}:{}: {:#}", host, port, java_err);
        }
    }

    // Fallback to Bedrock
    match ping_bedrock(host, port) {
        Ok(info) => {
            let result = PingResult {
                info: info.clone(),
                protocol: Protocol::Bedrock,
                cached_at: None,
            };
            if use_cache {
                cache_save(host, port, Protocol::Bedrock, &info, None, None);
            }
            return result;
        }
        Err(bedrock_err) => {
            log::debug!("Bedrock ping also failed for {}:{}: {:#}", host, port, bedrock_err);
        }
    }

    // Both failed — return a failed PingResult (callers should check `info.motd.is_empty()`)
    PingResult {
        info: ServerInfo {
            edition: String::new(),
            game_version: String::new(),
            protocol_version: 0,
            motd: String::new(),
            players_online: 0,
            players_max: 0,
            players: vec![],
            favicon: None,
        },
        protocol: Protocol::Java,
        cached_at: None,
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

/// Print a human-readable summary of server info to stdout.
pub fn print_server_info(result: &PingResult) {
    let info = &result.info;
    let edition_badge = match result.protocol {
        Protocol::Java    => "JAVA",
        Protocol::Bedrock => "BEDROCK",
    };
    println!("┌─ Minecraft Server Info ({}) ───────────────", edition_badge);
    println!("│ Version  : {} (protocol {})", info.game_version, info.protocol_version);
    println!("│ MOTD     : {}", info.motd);
    println!("│ Players  : {}/{}", info.players_online, info.players_max);
    if !info.players.is_empty() {
        let names = info.players.join(", ");
        println!("│ Online   : {}", names);
    }
    if let Some(ref f) = info.favicon {
        println!("│ Favicon  : ✓ ({} bytes)", f.len());
    }
    if let Some(ref cache) = result.cached_at {
        println!("│ Cache    : {}", cache);
    }
    println!("└────────────────────────────────────────────────");
}

// -------------------- Player-name monitor --------------------

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, SystemTime as _, UNIX_EPOCH as _};

/// Persistent storage for player names seen across monitor runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJournal {
    /// Host:port → set of player names ever seen
    pub seen: HashMap<String, BTreeSet<String>>,
    /// Host:port → first-seen timestamp
    pub first_seen: HashMap<String, u64>,
    /// Host:port → last-seen timestamp
    pub last_seen: HashMap<String, u64>,
}

impl Default for PlayerJournal {
    fn default() -> Self {
        Self {
            seen: HashMap::new(),
            first_seen: HashMap::new(),
            last_seen: HashMap::new(),
        }
    }
}

impl PlayerJournal {
    fn path() -> Result<PathBuf> {
        let base = directories::ProjectDirs::from("", "", "lambdaattack")
            .context("Cannot determine data directory")?
            .data_dir()
            .join("player-journal.json");
        Ok(base)
    }

    /// Load journal from disk.
    pub fn load() -> Self {
        let path = match Self::path() {
            Ok(p) => p,
            Err(_) => return Self::default(),
        };
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Save journal to disk.
    pub fn save(&self) {
        let path = match Self::path() {
            Ok(p) => p,
            Err(_) => return,
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json);
        }
    }

    /// Record the player names seen in this ping.
    pub fn record(&mut self, host: &str, port: u16, players: &[String]) {
        let key = format!("{}:{}", host, port);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = self.seen.entry(key.clone()).or_default();
        for name in players {
            if entry.insert(name.clone()) {
                // New player — log it
                log::info!("[journal] NEW PLAYER {} on {}:{}", name, host, port);
            }
        }
        self.first_seen.entry(key.clone()).or_insert(now);
        self.last_seen.insert(key, now);
    }

    /// Print a summary of all recorded players.
    pub fn print_summary(&self, host: &str, port: u16) {
        let key = format!("{}:{}", host, port);
        if let Some(players) = self.seen.get(&key) {
            println!("┌─ Player Journal ────────────────────────────");
            println!("│ Server  : {key}");
            println!("│ Total   : {} unique players", players.len());
            for name in players {
                let first = self.first_seen.get(&key).copied().unwrap_or(0);
                let last = self.last_seen.get(&key).copied().unwrap_or(0);
                println!("│ - {name}");
            }
            println!("└──────────────────────────────────────────────");
        }
    }
}

/// Run a continuous monitor loop: ping the server every `interval`, collect
/// player names, and persist them to the journal.
///
/// Press Ctrl+C to stop.
pub async fn monitor(
    host: &str,
    port: u16,
    protocol_version: i32,
    interval: Duration,
) -> Result<()> {
    let mut journal = PlayerJournal::load();
    log::info!("Starting monitor for {}:{} (interval={}s)", host, port, interval.as_secs());

    let mut ping_count = 0u64;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("Monitor interrupted by user");
                journal.save();
                journal.print_summary(host, port);
                break;
            }
            _ = tokio::time::sleep(interval) => {
                ping_count += 1;
                let result = ping(host, port, protocol_version, false);
                if !result.info.motd.is_empty() {
                    let new_players: Vec<&str> = result.info.players.iter()
                        .filter(|n| !journal.seen.get(&format!("{}:{}", host, port))
                            .map_or(false, |s| s.contains(*n)))
                        .map(|s| s.as_str())
                        .collect();

                    if !new_players.is_empty() {
                        log::info!(
                            "[#{ping_count}] {} new player(s): {}",
                            new_players.len(),
                            new_players.join(", ")
                        );
                    }

                    journal.record(host, port, &result.info.players);
                    journal.save();

                    log::info!(
                        "[#{ping_count}] {}:{} — {} {}/{} {:?}",
                        host, port,
                        result.info.game_version,
                        result.info.players_online,
                        result.info.players_max,
                        result.info.players,
                    );
                } else {
                    log::warn!("[#{ping_count}] Server unreachable — will retry");
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Utils
// ---------------------------------------------------------------------------

/// Strip Minecraft §-style colour codes and JSON formatting from MOTD text.
pub fn strip_motd_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            let _ = chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_motd_formatting() {
        assert_eq!(strip_motd_formatting("§aHello"), "Hello");
        assert_eq!(strip_motd_formatting("§x§x§x"), "");
        assert_eq!(strip_motd_formatting("Hello"), "Hello");
        assert_eq!(strip_motd_formatting("§lBold§rNormal"), "BoldNormal");
        assert_eq!(strip_motd_formatting("§cRed §bAqua"), "Red Aqua");
    }

    #[test]
    fn test_ping_java_localhost_timeout() {
        let result = ping_java("127.0.0.1", 25566, -1);
        assert!(result.is_err(), "Expected failure for offline server");
    }

    #[test]
    fn test_ping_bedrock_localhost_timeout() {
        let result = ping_bedrock("127.0.0.1", 19132);
        assert!(result.is_err(), "Expected failure for offline server");
    }

    #[test]
    fn test_cache_key_format() {
        let key = cache_key("mc.example.com", 25565);
        assert_eq!(key, "mc_example_com_25565.json");
    }

    #[test]
    fn test_ping_combined_both_fail() {
        // Both Java and Bedrock should fail on a random closed port
        let result = ping("127.0.0.1", 65535, -1, false);
        assert!(result.info.motd.is_empty(), "Expected empty result for unreachable server");
    }
}
