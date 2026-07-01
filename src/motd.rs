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
    let base = directories::ProjectDirs::from("", "", "creeper")
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

/// Minecraft-aware address resolution.
///
/// First tries SRV lookup (`_minecraft._tcp.<host>`) via `dig`,
/// then falls back to standard A/AAAA resolution.
/// Returns the resolved IP, the effective hostname (from SRV if applicable),
/// and the effective port.
fn resolve_mc(host: &str, port: u16) -> Result<(IpAddr, String, u16)> {
    // Try SRV record first
    if let Some((target, srv_port)) = lookup_srv(host) {
        log::debug!("SRV {} → {}:{}", host, target, srv_port);
        let ip = resolve_a(&target, srv_port)?;
        return Ok((ip, target, srv_port));
    }

    // Fallback: regular A/AAAA
    let ip = resolve_a(host, port)?;
    Ok((ip, host.to_owned(), port))
}

/// Try to resolve a Minecraft SRV record using `dig`.
fn lookup_srv(host: &str) -> Option<(String, u16)> {
    let srv_name = format!("_minecraft._tcp.{}", host);
    let output = std::process::Command::new("dig")
        .arg("+short")
        .arg("SRV")
        .arg(&srv_name)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let line = stdout.lines().next()?.trim().to_string();
    if line.is_empty() {
        return None;
    }
    // Format: "priority weight port target"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    let srv_port: u16 = parts.get(2)?.parse().ok()?;
    let target = parts[3].trim_end_matches('.');
    if target.is_empty() || target == "." {
        return None;
    }
    Some((target.to_string(), srv_port))
}

/// Basic A/AAAA resolution.
fn resolve_a(host: &str, port: u16) -> Result<IpAddr> {
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
    let (ip, resolved_host, resolved_port) = resolve_mc(host, port)?;
    let settings = RequestSettings { hostname: resolved_host.clone(), protocol_version };
    let resp = minecraft::query_java(&ip, Some(resolved_port), Some(settings))
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
    let (ip, _resolved_host, resolved_port) = resolve_mc(host, port)?;
    let resp = minecraft::query_bedrock(&ip, Some(resolved_port))
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

// -------------------- Player-name monitor (OSINT journal) --------------------

use std::collections::HashMap;
use std::time::{Duration, SystemTime as _, UNIX_EPOCH as _};

/// Social-engineering oriented player profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerEntry {
    pub name: String,
    /// 身份标签: 服主 / 管理员 / 普通玩家 / 未知
    pub role: String,
    /// 自由备注: 发现上下文、行为特征等
    pub note: String,
    /// First seen (UNIX timestamp)
    pub first_seen: u64,
    /// Last seen (UNIX timestamp)
    pub last_seen: u64,
}

/// Persistent storage for player names and OSINT metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJournal {
    /// Host:port → list of known players
    pub players: HashMap<String, Vec<PlayerEntry>>,
}

impl Default for PlayerJournal {
    fn default() -> Self {
        Self { players: HashMap::new() }
    }
}

impl PlayerJournal {
    fn path() -> Result<PathBuf> {
        let base = directories::ProjectDirs::from("", "", "creeper")
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
        let data = match fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => return Self::default(),
        };
        // Try new format first
        if let Ok(j) = serde_json::from_str::<Self>(&data) {
            return j;
        }
        // Try migrating from old format
        if let Ok(old) = serde_json::from_str::<OldJournal>(&data) {
            return Self::migrate_from_old(old);
        }
        Self::default()
    }

    /// Migrate from the old flat-name format.
    fn migrate_from_old(old: OldJournal) -> Self {
        let mut players: HashMap<String, Vec<PlayerEntry>> = HashMap::new();
        for (key, names) in old.seen {
            let first = old.first_seen.get(&key).copied().unwrap_or(0);
            let last = old.last_seen.get(&key).copied().unwrap_or(0);
            let entries: Vec<PlayerEntry> = names
                .into_iter()
                .map(|name| PlayerEntry {
                    name,
                    role: "未知".into(),
                    note: String::new(),
                    first_seen: first,
                    last_seen: last,
                })
                .collect();
            players.insert(key, entries);
        }
        Self { players }
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
    /// New names get "未知" role; existing names update last_seen.
    pub fn record(&mut self, host: &str, port: u16, names: &[String]) {
        let key = format!("{}:{}", host, port);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entries = self.players.entry(key.clone()).or_default();

        for name in names {
            if let Some(existing) = entries.iter_mut().find(|e| e.name == *name) {
                // Already known — update last_seen
                existing.last_seen = now;
            } else {
                // New discovery
                log::info!("[journal] NEW DISCOVERY {} on {}:{}", name, host, port);
                entries.push(PlayerEntry {
                    name: name.clone(),
                    role: "未知".into(),
                    note: String::new(),
                    first_seen: now,
                    last_seen: now,
                });
            }
        }
    }

    /// Print a summary of all recorded players.
    pub fn print_summary(&self, host: &str, port: u16) {
        let key = format!("{}:{}", host, port);
        if let Some(entries) = self.players.get(&key) {
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

    /// Merge another journal into this one. Newer last_seen wins; unknown names are added.
    pub fn merge(&mut self, other: &PlayerJournal) -> usize {
        let mut added = 0usize;
        for (key, entries) in &other.players {
            let local = self.players.entry(key.clone()).or_default();
            for remote_entry in entries {
                if let Some(local_entry) = local.iter_mut().find(|e| e.name == remote_entry.name) {
                    // Update last_seen if remote is newer
                    if remote_entry.last_seen > local_entry.last_seen {
                        local_entry.last_seen = remote_entry.last_seen;
                    }
                    // Keep the more informative role / note
                    if local_entry.role == "未知" && remote_entry.role != "未知" {
                        local_entry.role = remote_entry.role.clone();
                    }
                    if local_entry.note.is_empty() && !remote_entry.note.is_empty() {
                        local_entry.note = remote_entry.note.clone();
                    }
                } else {
                    // New player
                    local.push(remote_entry.clone());
                    added += 1;
                }
            }
        }
        if added > 0 {
            log::info!("[journal] Merged {} new player(s) from remote", added);
        }
        added
    }

    /// Update the role and note for a known player.
    pub fn annotate(&mut self, host: &str, port: u16, player: &str, role: &str, note: &str) {
        let key = format!("{}:{}", host, port);
        if let Some(entries) = self.players.get_mut(&key) {
            if let Some(entry) = entries.iter_mut().find(|e| e.name == player) {
                if !role.is_empty() { entry.role = role.to_owned(); }
                if !note.is_empty() { entry.note = note.to_owned(); }
                log::info!("[journal] ANNOTATED {} on {} -> role={}, note={}", player, key, entry.role, entry.note);
            }
        }
    }
}

/// Old journal format for migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OldJournal {
    seen: HashMap<String, Vec<String>>,
    first_seen: HashMap<String, u64>,
    last_seen: HashMap<String, u64>,
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
                        .filter(|n| !journal.players.get(&format!("{}:{}", host, port))
                            .map_or(false, |entries| entries.iter().any(|e| e.name == **n)))
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

/// Same as `monitor()` but pushes the journal to a sync hub after each ping.
pub async fn monitor_sync(
    host: &str,
    port: u16,
    protocol_version: i32,
    interval: Duration,
    sync_url: Option<&str>,
) -> Result<()> {
    let mut journal = PlayerJournal::load();
    log::info!("Starting monitor for {}:{} (interval={}s)", host, port, interval.as_secs());

    // If a sync hub is configured, pull remote journal first for a quick merge
    if let Some(url) = sync_url {
        if let Some(remote) = crate::sync::pull_from_hub(url).await {
            journal.merge(&remote);
            journal.save();
        }
    }

    let mut ping_count = 0u64;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("Monitor interrupted by user");
                journal.save();
                // Final push to hub before exit
                if let Some(url) = sync_url {
                    crate::sync::push_to_hub(url, &journal).await;
                }
                journal.print_summary(host, port);
                break;
            }
            _ = tokio::time::sleep(interval) => {
                ping_count += 1;
                let result = ping(host, port, protocol_version, false);
                if !result.info.motd.is_empty() {
                    journal.record(host, port, &result.info.players);
                    journal.save();

                    // Push to hub
                    if let Some(url) = sync_url {
                        crate::sync::push_to_hub(url, &journal).await;
                    }

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
// Multi-target monitor
// ---------------------------------------------------------------------------

/// A single monitoring target read from a JSON targets file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorTarget {
    pub provider: String,
    pub host: String,
    pub port: u16,
    /// Ping interval in seconds (default 60)
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

fn default_interval() -> u64 { 60 }

/// Top-level structure of the targets JSON file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TargetsFile {
    targets: Vec<MonitorTarget>,
}

/// Monitor all targets from a JSON file, syncing to the given hub URL.
/// Each target runs its own monitoring loop concurrently.
pub async fn monitor_all(targets_path: &str, sync_url: Option<&str>) -> Result<()> {
    let data = fs::read_to_string(targets_path)
        .context(format!("Failed to read targets file: {}", targets_path))?;
    let file: TargetsFile = serde_json::from_str(&data)
        .context("Failed to parse targets file")?;

    if file.targets.is_empty() {
        anyhow::bail!("No targets found in {}", targets_path);
    }

    log::info!("Loaded {} monitoring targets from {}", file.targets.len(), targets_path);
    for t in &file.targets {
        log::info!("  {}:{} ({}) — every {}s",
            t.host, t.port, t.provider, t.interval_secs);
    }

    let mut handles = Vec::new();
    for target in file.targets {
        let host = target.host.clone();
        let port = target.port;
        let interval = Duration::from_secs(target.interval_secs);
        let url = sync_url.map(|s| s.to_string());

        handles.push(tokio::spawn(async move {
            log::info!("[monitor-all] starting {}:{}", host, port);
            let url_deref = url.as_deref();
            if let Err(e) = monitor_sync(&host, port, -1, interval, url_deref).await {
                log::error!("[monitor-all] {}:{} failed: {e}", host, port);
            }
        }));
    }

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    log::info!("Shutting down all monitors...");

    // Abort all tasks
    for h in handles {
        h.abort();
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
