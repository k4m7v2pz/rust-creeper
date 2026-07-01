//! Network scanner — IPv4 range + port scanning with protocol detection.
//!
//! Supports:
//!   - IP formats: single (1.1.1.1), CIDR (1.1.1.0/24), range (1.1.1.1~1.1.1.254)
//!   - Port formats: single (80), range (10000~50000)
//!   - Scan modes: tcp, udp, http, icmp, rdp, ssh

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

// -------------------- IP range parsing --------------------

/// Parse an IPv4 target string (single, CIDR, or range like 1.1.1.1~1.1.1.254).
pub fn parse_ip_range(input: &str) -> Result<Vec<Ipv4Addr>, String> {
    // CIDR: 1.1.1.0/24
    if let Some((ip_str, bits_str)) = input.split_once('/') {
        let base: Ipv4Addr = ip_str.parse().map_err(|e| format!("bad IP: {e}"))?;
        let bits: u8 = bits_str.parse().map_err(|_| "bad CIDR prefix".to_string())?;
        if bits > 32 {
            return Err("CIDR prefix must be 0-32".into());
        }
        let mask = if bits == 0 { 0u32 } else { !0u32 << (32 - bits) };
        let base_u32 = u32::from(base) & mask;
        let count = if bits == 32 { 1usize } else { 1 << (32 - bits) };
        let mut ips = Vec::with_capacity(count);
        for i in 0..count as u32 {
            ips.push(Ipv4Addr::from(base_u32 | i));
        }
        // Remove network/broadcast for real subnets (> /31)
        if count > 2 {
            return Ok(ips[1..ips.len() - 1].to_vec());
        }
        return Ok(ips);
    }

    // Range: 1.1.1.1~1.1.1.254
    if let Some((start_str, end_str)) = input.split_once('~') {
        let start: Ipv4Addr = start_str.parse().map_err(|e| format!("bad IP: {e}"))?;
        let end: Ipv4Addr = end_str.parse().map_err(|e| format!("bad IP: {e}"))?;
        if u32::from(start) > u32::from(end) {
            return Err("start IP > end IP".into());
        }
        let count = (u32::from(end) - u32::from(start) + 1) as usize;
        let mut ips = Vec::with_capacity(count);
        let mut cur = u32::from(start);
        while cur <= u32::from(end) {
            ips.push(Ipv4Addr::from(cur));
            cur += 1;
        }
        return Ok(ips);
    }

    // Single IP
    let ip: Ipv4Addr = input.parse().map_err(|e| format!("bad IP: {e}"))?;
    Ok(vec![ip])
}

// -------------------- Port range parsing --------------------

/// Parse port spec: single (80), range (10000~50000), or comma list.
pub fn parse_ports(input: &str) -> Result<Vec<u16>, String> {
    let mut ports = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((lo_str, hi_str)) = part.split_once('~') {
            let lo: u16 = lo_str.parse().map_err(|_| format!("bad port: {lo_str}"))?;
            let hi: u16 = hi_str.parse().map_err(|_| format!("bad port: {hi_str}"))?;
            if lo > hi {
                return Err(format!("port range {lo} > {hi}"));
            }
            for p in lo..=hi {
                ports.push(p);
            }
        } else {
            let p: u16 = part.parse().map_err(|_| format!("bad port: {part}"))?;
            ports.push(p);
        }
    }
    Ok(ports)
}

// -------------------- Scan result --------------------

#[derive(Debug, Clone)]
pub struct PortResult {
    pub ip: Ipv4Addr,
    pub port: u16,
    pub state: PortState,
    pub protocol: Option<String>,
    pub banner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortState {
    Open,
    Closed,
    Filtered,
}

impl PortState {
    pub fn symbol(&self) -> &str {
        match self {
            PortState::Open => "✓",
            PortState::Closed => "✗",
            PortState::Filtered => "?",
        }
    }
}

// -------------------- Scan functions --------------------

pub const TCP_TIMEOUT: Duration = Duration::from_secs(2);
pub const UDP_TIMEOUT: Duration = Duration::from_secs(2);

/// TCP connect scan — just try to connect.
pub async fn scan_tcp(ip: Ipv4Addr, port: u16) -> PortResult {
    let addr = SocketAddrV4::new(ip, port);
    match timeout(TCP_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(Ok(_)) => PortResult {
            ip,
            port,
            state: PortState::Open,
            protocol: Some("tcp".into()),
            banner: None,
        },
        Ok(Err(_)) => PortResult {
            ip,
            port,
            state: PortState::Closed,
            protocol: Some("tcp".into()),
            banner: None,
        },
        Err(_) => PortResult {
            ip,
            port,
            state: PortState::Filtered,
            protocol: Some("tcp".into()),
            banner: None,
        },
    }
}

/// UDP probe — send a small datagram, wait for ICMP unreachable or response.
/// Uses tokio UDP socket with timeout.
pub async fn scan_udp(ip: Ipv4Addr, port: u16) -> PortResult {
    let addr = SocketAddrV4::new(ip, port);
    match timeout(UDP_TIMEOUT, async {
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(addr).await?;
        // Send empty probe datagram
        socket.send(&[]).await?;
        // Try to receive — ICMP unreachable comes as error, response as data
        let mut buf = [0u8; 512];
        match timeout(Duration::from_secs(1), socket.recv(&mut buf)).await {
            Ok(Ok(n)) => Ok::<_, std::io::Error>(Some(n)),
            _ => Ok(None),
        }
    })
    .await
    {
        Ok(Ok(Some(_))) => PortResult {
            ip,
            port,
            state: PortState::Open,
            protocol: Some("udp".into()),
            banner: None,
        },
        Ok(Ok(None)) => PortResult {
            ip,
            port,
            state: PortState::Open, // No response but no error = likely open
            protocol: Some("udp".into()),
            banner: None,
        },
        Ok(Err(_)) => PortResult {
            ip,
            port,
            state: PortState::Closed,
            protocol: Some("udp".into()),
            banner: None,
        },
        Err(_) => PortResult {
            ip,
            port,
            state: PortState::Filtered,
            protocol: Some("udp".into()),
            banner: None,
        },
    }
}

/// HTTP probe — GET / and check response.
pub async fn scan_http(ip: Ipv4Addr, port: u16) -> PortResult {
    let url = format!("http://{}:{}/", ip, port);
    match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
    {
        Ok(client) => match client.get(&url).send().await {
            Ok(resp) => {
                let server = resp
                    .headers()
                    .get("server")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                PortResult {
                    ip,
                    port,
                    state: PortState::Open,
                    protocol: Some("http".into()),
                    banner: server,
                }
            }
            Err(_) => {
                // Maybe HTTPS on this port
                let url_https = format!("https://{}:{}/", ip, port);
                match client.get(&url_https).send().await {
                    Ok(resp) => {
                        let server = resp
                            .headers()
                            .get("server")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string());
                        PortResult {
                            ip,
                            port,
                            state: PortState::Open,
                            protocol: Some("https".into()),
                            banner: server,
                        }
                    }
                    Err(_) => PortResult {
                        ip,
                        port,
                        state: PortState::Closed,
                        protocol: Some("http".into()),
                        banner: None,
                    },
                }
            }
        },
        Err(_) => PortResult {
            ip,
            port,
            state: PortState::Closed,
            protocol: Some("http".into()),
            banner: None,
        },
    }
}

/// ICMP ping — fork system `ping -c 1 -W 2`.
pub async fn scan_icmp(ip: Ipv4Addr) -> PortResult {
    let ip_str = ip.to_string();
    let output = tokio::process::Command::new("ping")
        .args(["-c", "1", "-W", "2", &ip_str])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => PortResult {
            ip,
            port: 0,
            state: PortState::Open,
            protocol: Some("icmp".into()),
            banner: None,
        },
        _ => PortResult {
            ip,
            port: 0,
            state: PortState::Filtered,
            protocol: Some("icmp".into()),
            banner: None,
        },
    }
}

/// RDP probe — TCP connect to 3389, read initial TPKT header.
pub async fn scan_rdp(ip: Ipv4Addr, port: u16) -> PortResult {
    let addr = SocketAddrV4::new(ip, port);
    match timeout(Duration::from_secs(3), async {
        let mut stream = TcpStream::connect(addr).await?;
        let mut buf = [0u8; 8];
        match timeout(Duration::from_secs(2), tokio::io::AsyncReadExt::read(&mut stream, &mut buf))
            .await
        {
            Ok(Ok(n)) if n >= 3 && buf[0] == 0x03 => Ok::<_, std::io::Error>(true), // TPKT version 3
            _ => Ok(false),
        }
    })
    .await
    {
        Ok(Ok(true)) => PortResult {
            ip,
            port,
            state: PortState::Open,
            protocol: Some("rdp".into()),
            banner: None,
        },
        Ok(Ok(false)) => PortResult {
            ip,
            port,
            state: PortState::Open, // TCP open but not RDP TPKT
            protocol: Some("rdp".into()),
            banner: None,
        },
        Ok(Err(_)) => PortResult {
            ip,
            port,
            state: PortState::Closed,
            protocol: Some("rdp".into()),
            banner: None,
        },
        Err(_) => PortResult {
            ip,
            port,
            state: PortState::Filtered,
            protocol: Some("rdp".into()),
            banner: None,
        },
    }
}

/// SSH probe — TCP connect, read SSH banner (b'SSH-').
pub async fn scan_ssh(ip: Ipv4Addr, port: u16) -> PortResult {
    let addr = SocketAddrV4::new(ip, port);
    match timeout(Duration::from_secs(3), async {
        let mut stream = TcpStream::connect(addr).await?;
        let mut buf = [0u8; 256];
        match timeout(Duration::from_secs(2), tokio::io::AsyncReadExt::read(&mut stream, &mut buf))
            .await
        {
            Ok(Ok(n)) if n > 0 => {
                let banner = String::from_utf8_lossy(&buf[..n]);
                if banner.starts_with("SSH-") {
                    Ok::<_, std::io::Error>(Some(banner.trim_end().to_string()))
                } else {
                    Ok(Some(banner.trim().to_string()))
                }
            }
            _ => Ok(None),
        }
    })
    .await
    {
        Ok(Ok(Some(banner))) => PortResult {
            ip,
            port,
            state: PortState::Open,
            protocol: Some(if banner.starts_with("SSH-") { "ssh" } else { "tcp" }.into()),
            banner: Some(banner),
        },
        Ok(Ok(None)) => PortResult {
            ip,
            port,
            state: PortState::Open,
            protocol: Some("tcp".into()),
            banner: None,
        },
        Ok(Err(_)) => PortResult {
            ip,
            port,
            state: PortState::Closed,
            protocol: Some("ssh".into()),
            banner: None,
        },
        Err(_) => PortResult {
            ip,
            port,
            state: PortState::Filtered,
            protocol: Some("ssh".into()),
            banner: None,
        },
    }
}

/// Run a full scan job: iterate IPs × ports × scan modes concurrently.
pub async fn run_scan(
    targets: Vec<Ipv4Addr>,
    ports: Vec<u16>,
    modes: &[String],
    concurrency: usize,
) -> Vec<PortResult> {
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    let sem = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::new();

    // Capture ports length before the loop (avoid borrow-after-move in closures)
    let ports_len = ports.len();

    for &ip in &targets {
        for &port in &ports {
            for mode in modes {
                let mode = mode.clone();
                let permit = sem.clone();
                handles.push(tokio::spawn(async move {
                    let _guard = permit.acquire().await;
                    match mode.as_str() {
                        "tcp" => scan_tcp(ip, port).await,
                        "udp" => scan_udp(ip, port).await,
                        "http" => scan_http(ip, port).await,
                        "rdp" => if port == 3389 || ports_len <= 10 {
                            scan_rdp(ip, port).await
                        } else {
                            scan_tcp(ip, port).await // fallback
                        },
                        "ssh" => if port == 22 || ports_len <= 10 {
                            scan_ssh(ip, port).await
                        } else {
                            scan_tcp(ip, port).await
                        },
                        _ => scan_tcp(ip, port).await,
                    }
                }));
            }
        }
    }

    // ICMP is per-host, not per-port
    if modes.contains(&"icmp".to_string()) {
        for &ip in &targets {
            let permit = sem.clone();
            handles.push(tokio::spawn(async move {
                let _guard = permit.acquire().await;
                scan_icmp(ip).await
            }));
        }
    }

    let mut results: Vec<PortResult> = Vec::new();
    let total = handles.len();
    for h in handles {
        match h.await {
            Ok(r) => results.push(r),
            Err(e) => log::warn!("scan task panicked: {e}"),
        }
    }
    log::info!(
        "Scan complete: {} targets × {} port(s) → {} results ({:.1}%)",
        targets.len(),
        ports.len(),
        results.len(),
        results.len() as f64 / total as f64 * 100.0
    );

    results
}

/// Print scan results in a human-readable format.
pub fn print_results(results: &[PortResult]) {
    if results.is_empty() {
        println!("(no results)");
        return;
    }

    // Group by IP
    use std::collections::BTreeMap;
    let mut by_ip: BTreeMap<String, Vec<&PortResult>> = BTreeMap::new();
    for r in results {
        by_ip.entry(r.ip.to_string()).or_default().push(r);
    }

    for (ip, entries) in &by_ip {
        for r in entries {
            let state_icon = match r.state {
                PortState::Open => "✓",
                PortState::Closed => "✗",
                PortState::Filtered => "?",
            };
            let proto = r.protocol.as_deref().unwrap_or("-");
            let banner = r
                .banner
                .as_deref()
                .map(|b| format!("  [{b}]"))
                .unwrap_or_default();
            let port_str = if r.port == 0 {
                    String::new()
                } else {
                    format!(":{:<5}", r.port)
                };
            println!(
                "  {:<15} {:<6} {:<6} {}{}",
                ip, port_str, proto, state_icon, banner
            );
        }
    }

    // Summary
    let open = results.iter().filter(|r| r.state == PortState::Open).count();
    let closed = results.iter().filter(|r| r.state == PortState::Closed).count();
    let filtered = results
        .iter()
        .filter(|r| r.state == PortState::Filtered)
        .count();
    println!();
    println!(
        "Summary: {} open, {} closed, {} filtered ({} total)",
        open,
        closed,
        filtered,
        results.len()
    );
}
