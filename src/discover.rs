//! MC server discovery — scan domain patterns + port ranges to find MC servers.
//!
//! Typical usage:
//!   creeper discover --domains "srv{}.example.com:0..4" --ports "25565,10000-10500"

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::time::timeout;

/// One discovered MC server.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredServer {
    pub host: String,
    pub port: u16,
    pub game_version: String,
    pub players_online: u32,
    pub players_max: u32,
    pub players: Vec<String>,
    pub motd: String,
    pub edition: String,
}

/// A domain pattern to scan, e.g. "srv{}.example.com" with range `start..=end`.
#[derive(Debug, Clone)]
pub struct DomainPattern {
    /// Template with `{}` placeholder, e.g. "srv{}.example.com"
    pub template: String,
    /// Numeric range to substitute into `{}`
    pub start: u32,
    pub end: u32,
}

impl DomainPattern {
    /// Parse from CLI syntax:
    ///   "example.com"             → static single host
    ///   "srv{}.example.com:0..50"  → template with range
    pub fn parse(input: &str) -> Result<Self> {
        // Static domain (no colon → no range)
        if !input.contains(':') {
            return Ok(Self {
                template: input.to_string(),
                start: 0,
                end: 0,
            });
        }

        let (template, range_str) = input
            .rsplit_once(':')
            .context("expected format 'template:start..end', e.g. 'srv{}.example.com:1..50'")?;

        let (start_str, end_str) = range_str
            .split_once("..")
            .context("range must be 'start..end', e.g. '0..50'")?;

        let start: u32 = start_str.parse().context("invalid start number")?;
        let end: u32 = end_str.parse().context("invalid end number")?;

        if !template.contains("{}") {
            anyhow::bail!("template must contain '{{}}' placeholder, or omit ':' for a static domain");
        }

        Ok(Self {
            template: template.to_string(),
            start,
            end,
        })
    }

    /// Generate all candidate hostnames.
    pub fn candidates(&self) -> Vec<String> {
        (self.start..=self.end)
            .map(|n| self.template.replace("{}", &n.to_string()))
            .collect()
    }
}

/// Parse port list from CLI: "25565,10000,19420,25000-26000"
pub fn parse_ports(input: &str) -> Result<Vec<u16>> {
    let mut ports = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((lo, hi)) = part.split_once('-') {
            let lo: u16 = lo.parse().context("invalid port")?;
            let hi: u16 = hi.parse().context("invalid port")?;
            ports.extend(lo..=hi);
        } else {
            ports.push(part.parse().context("invalid port")?);
        }
    }
    ports.sort();
    ports.dedup();
    Ok(ports)
}

/// Resolve a hostname to its IPv4 addresses (with 3s timeout).
async fn resolve(host: &str) -> Option<Vec<SocketAddr>> {
    match timeout(
        Duration::from_secs(3),
        tokio::net::lookup_host(format!("{}:0", host)),
    )
    .await
    {
        Ok(Ok(addrs)) => {
            let v: Vec<_> = addrs.collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        _ => {
            log::debug!("DNS failed for {}: timeout or error", host);
            None
        }
    }
}

/// Check if a TCP port is reachable (fast connect, no data exchange).
async fn tcp_probe(addr: SocketAddr) -> bool {
    match timeout(Duration::from_secs(2), TcpStream::connect(addr)).await {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

/// Discover MC servers by scanning domain patterns on given ports.
///
/// For each candidate hostname:
/// 1. Resolve DNS
/// 2. TCP-probe each port
/// 3. If TCP connects, do full MOTD ping to verify it's an MC server
pub async fn discover(
    patterns: &[DomainPattern],
    ports: &[u16],
    concurrency: usize,
) -> Vec<DiscoveredServer> {
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    // Build candidate list: (hostname, port)
    let mut candidates: Vec<(String, u16)> = Vec::new();
    for pat in patterns {
        for host in pat.candidates() {
            for &port in ports {
                candidates.push((host.clone(), port));
            }
        }
    }

    log::info!(
        "Discover: {} patterns → {} hostnames × {} ports = {} candidates",
        patterns.len(),
        patterns.iter().map(|p| p.candidates().len()).sum::<usize>(),
        ports.len(),
        candidates.len(),
    );

    let sem = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::new();

    // Group by hostname to avoid re-resolving DNS
    let mut by_host: BTreeMap<String, Vec<u16>> = BTreeMap::new();
    for (host, port) in &candidates {
        by_host.entry(host.clone()).or_default().push(*port);
    }

    let total_hosts = by_host.len();
    let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    for (host, port_list) in by_host {
        let sem = sem.clone();
        let ports = ports.to_vec(); // for logging
        let done_ref = Arc::clone(&done);
        handles.push(tokio::spawn(async move {
            let _guard = sem.acquire().await;

            // DNS
            let addrs = match resolve(&host).await {
                Some(a) => a,
                None => {
                    log::debug!("[dns-fail] {}", host);
                    done_ref.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Vec::new();
                }
            };

            // Pick first IPv4
            let addr = match addrs.iter().find(|a| a.is_ipv4()) {
                Some(a) => *a,
                None => {
                    log::debug!("[no-ipv4] {}", host);
                    done_ref.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Vec::new();
                }
            };

            let mut found = Vec::new();
            for &port in &port_list {
                let sock = SocketAddr::new(addr.ip(), port);

                // Step 1: fast TCP probe
                if !tcp_probe(sock).await {
                    continue;
                }

                // Step 2: MOTD ping to verify it's MC (with 5s timeout)
                let result = timeout(
                    Duration::from_secs(5),
                    tokio::task::spawn_blocking({
                        let host = host.clone();
                        move || crate::motd::ping(&host, port, -1, false)
                    }),
                )
                .await;

                match result {
                    Ok(Ok(ping_result)) if !ping_result.info.motd.is_empty() => {
                        log::info!(
                            "[FOUND] {}:{} — {} {}/{}",
                            host,
                            port,
                            ping_result.info.game_version,
                            ping_result.info.players_online,
                            ping_result.info.players_max,
                        );
                        found.push(DiscoveredServer {
                            host: host.clone(),
                            port,
                            game_version: ping_result.info.game_version,
                            players_online: ping_result.info.players_online,
                            players_max: ping_result.info.players_max,
                            players: ping_result.info.players,
                            motd: ping_result.info.motd,
                            edition: ping_result.info.edition,
                        });
                    }
                    Ok(_) => {
                        log::debug!("[tcp-only] {}:{} (not MC)", host, port);
                    }
                    Err(e) => {
                        log::debug!("[spawn-err] {}:{} {:#}", host, port, e);
                    }
                }
            }

            let n = done_ref.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n % 10 == 0 || n == total_hosts {
                log::info!("Progress: {}/{} hostnames scanned", n, total_hosts);
            }

            found
        }));
    }

    // Collect results
    let mut all: Vec<DiscoveredServer> = Vec::new();
    for h in handles {
        match h.await {
            Ok(servers) => all.extend(servers),
            Err(e) => log::warn!("discover task panicked: {e}"),
        }
    }

    // Sort by hostname then port
    all.sort_by(|a, b| a.host.cmp(&b.host).then(a.port.cmp(&b.port)));

    all
}

/// Print discovery results in a nice table.
pub fn print_results(servers: &[DiscoveredServer]) {
    if servers.is_empty() {
        println!("No MC servers found.");
        return;
    }

    println!("\n=== Found {} MC server(s) ===\n", servers.len());
    for s in servers {
        println!(
            "  {:40} {:>6}  {:20}  {:>3}/{:>3}  {}",
            s.host,
            s.port,
            s.game_version,
            s.players_online,
            s.players_max,
            if s.players.is_empty() {
                String::new()
            } else {
                format!("[{}]", s.players.join(", "))
            }
        );
    }
    println!();
}

/// Generate JSON output for appending to a targets file (e.g. targets-template.json).
pub fn to_targets_json(servers: &[DiscoveredServer]) -> String {
    let entries: Vec<String> = servers
        .iter()
        .map(|s| {
            format!(
                r#"    {{"provider": "discovered", "host": "{}", "port": {}, "interval_secs": 60}}"#,
                s.host, s.port
            )
        })
        .collect();
    entries.join(",\n")
}
