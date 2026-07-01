//! Multi-node sync layer: lightweight HTTP Hub for journal exchange.
//!
//! Hub mode (`creeper serve`):
//!   GET  /journal — return full PlayerJournal JSON
//!   POST /merge   — receive a journal and merge into local one
//!   GET  /status  — health check + uptime
//!
//! Client mode (`creeper monitor --sync <hub>`, `creeper tui --remote <hub>`):
//!   Periodically push / pull journal to/from a Hub.

use crate::motd::PlayerJournal;
use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

// Json wrapper with explicit UTF-8 charset (fixes Safari garbled text)
struct Utf8Json<T: Serialize>(T);

impl<T: Serialize> IntoResponse for Utf8Json<T> {
    fn into_response(self) -> axum::response::Response {
        let mut resp = Json(self.0).into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            "application/json; charset=utf-8".parse().unwrap(),
        );
        resp
    }
}

// -------------------- shared state --------------------

pub struct HubState {
    pub journal: Mutex<PlayerJournal>,
    pub started_at: Instant,
    /// Host:port being monitored (if any)
    pub target: String,
}

// -------------------- API handlers --------------------

async fn get_journal(State(state): State<Arc<HubState>>) -> Utf8Json<PlayerJournal> {
    let j = state.journal.lock().unwrap();
    Utf8Json(j.clone())
}

async fn post_merge(
    State(state): State<Arc<HubState>>,
    Json(remote): Json<PlayerJournal>,
) -> Utf8Json<serde_json::Value> {
    let added = state.journal.lock().unwrap().merge(&remote);
    // Persist after merge
    state.journal.lock().unwrap().save();
    Utf8Json(serde_json::json!({
        "merged": true,
        "new_players": added
    }))
}

async fn get_status(State(state): State<Arc<HubState>>) -> Utf8Json<serde_json::Value> {
    let uptime = state.started_at.elapsed().as_secs();
    let player_count = state
        .journal
        .lock()
        .unwrap()
        .players
        .values()
        .map(|v| v.len())
        .sum::<usize>();
    Utf8Json(serde_json::json!({
        "status": "ok",
        "uptime_secs": uptime,
        "target": state.target,
        "total_players": player_count,
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Root endpoint — shows hub info + connection details
async fn get_root(
    State(state): State<Arc<HubState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Utf8Json<serde_json::Value> {
    let uptime = state.started_at.elapsed().as_secs();
    let players: Vec<_> = state
        .journal
        .lock()
        .unwrap()
        .players
        .iter()
        .flat_map(|(srv, entries)| {
            entries.iter().map(move |e| serde_json::json!({
                "server": srv,
                "name": e.name,
                "role": e.role,
            }))
        })
        .collect();

    // --- connection metadata ---
    let client_ip = addr.ip().to_string();
    let client_port = addr.port();

    // Parse User-Agent → browser / kernel
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    let (browser, kernel) = parse_user_agent(ua);

    // Detect browser vs programmatic call
    let is_browser = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|a| a.contains("text/html"))
        .unwrap_or(false);
    let access_type = if is_browser { "browser" } else { "api" };

    Utf8Json(serde_json::json!({
        "service": "Creeper Hub",
        "version": env!("CARGO_PKG_VERSION"),
        "target": state.target,
        "uptime_secs": uptime,
        "total_players": players.len(),
        "connection": {
            "client_ip": client_ip,
            "client_port": client_port,
            "access_type": access_type,
            "browser": browser,
            "kernel": kernel,
            "raw_ua": ua,
        },
        "endpoints": {
            "/":         "this page",
            "/journal":  "GET — full player journal",
            "/merge":    "POST — merge remote journal",
            "/status":   "GET — health & stats",
        },
        "players": players,
    }))
}

/// Simple User-Agent parser — returns (browser_name, kernel_name)
fn parse_user_agent(ua: &str) -> (&'static str, &'static str) {
    let browser = if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera"
    } else if ua.contains("Firefox/") && !ua.contains("Seamonkey/") {
        "Firefox"
    } else if ua.contains("Chrome/") && !ua.contains("Edg/") && !ua.contains("OPR/") {
        "Chrome"
    } else if ua.contains("Safari/") && !ua.contains("Chrome/") && !ua.contains("Chromium/") {
        "Safari"
    } else if ua.contains("curl/") {
        "curl"
    } else if ua.contains("reqwest/") {
        "reqwest (Rust)"
    } else if ua.contains("python-requests") || ua.contains("Python/") {
        "Python requests"
    } else if ua.contains("Go-http-client") {
        "Go"
    } else {
        "unknown"
    };

    let kernel = if ua.contains("WebKit/") || ua.contains("AppleWebKit/") {
        "WebKit"
    } else if ua.contains("Blink/") || (ua.contains("Chrome/") && !ua.contains("Safari/")) {
        "Blink"
    } else if ua.contains("Gecko/") {
        "Gecko"
    } else if ua.contains("Trident/") {
        "Trident"
    } else {
        "—"
    };

    (browser, kernel)
}

// -------------------- Hub server --------------------

pub async fn run_hub(host: &str, port: u16, target: &str) -> anyhow::Result<()> {
    let state = Arc::new(HubState {
        journal: Mutex::new(PlayerJournal::load()),
        started_at: Instant::now(),
        target: target.to_string(),
    });

    let app = Router::new()
        .route("/", get(get_root))
        .route("/journal", get(get_journal))
        .route("/merge", post(post_merge))
        .route("/status", get(get_status))
        .with_state(state.clone());

    let addr = format!("{}:{}", host, port);
    log::info!("[sync] Hub listening on http://{addr}");
    println!("Creeper Hub listening on http://{addr}");
    println!("  GET  /journal  — full player journal");
    println!("  POST /merge    — merge remote journal");
    println!("  GET  /status   — health & stats");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

// -------------------- sync client (probe) --------------------

/// Push the current journal to a Hub.
pub async fn push_to_hub(hub_url: &str, journal: &PlayerJournal) {
    let url = format!("{}/merge", hub_url.trim_end_matches('/'));
    match reqwest::Client::new()
        .post(&url)
        .json(journal)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let added = body.get("new_players").and_then(|v| v.as_u64()).unwrap_or(0);
                    if added > 0 {
                        log::info!("[sync] Pushed journal to hub, {} new players merged", added);
                    }
                }
            } else {
                log::warn!("[sync] Hub returned {}", resp.status());
            }
        }
        Err(e) => {
            log::warn!("[sync] Failed to push to hub {}: {}", hub_url, e);
        }
    }
}

/// Pull the full journal from a Hub.
pub async fn pull_from_hub(hub_url: &str) -> Option<PlayerJournal> {
    let url = format!("{}/journal", hub_url.trim_end_matches('/'));
    match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<PlayerJournal>().await {
                    Ok(j) => Some(j),
                    Err(e) => {
                        log::warn!("[sync] Failed to parse journal from hub: {}", e);
                        None
                    }
                }
            } else {
                log::warn!("[sync] Hub returned {} for GET /journal", resp.status());
                None
            }
        }
        Err(e) => {
            log::warn!("[sync] Failed to pull from hub {}: {}", hub_url, e);
            None
        }
    }
}

/// Query a Hub and print a human-readable node status summary.
pub async fn print_node_status(hub_url: &str) {
    // Auto-prepend http:// if no scheme present
    let base = if hub_url.contains("://") {
        hub_url.trim_end_matches('/').to_string()
    } else {
        format!("http://{}", hub_url.trim_end_matches('/'))
    };

    // Fetch status
    print!("Connecting to {base} ... ");
    let status = pull_status(&base).await;
    match status {
        Some(ref s) => println!("{} (v{})", s["status"].as_str().unwrap_or("?"), s["version"].as_str().unwrap_or("?")),
        None => { println!("❌ unreachable"); return; }
    }
    let status = status.unwrap();
    let uptime = status["uptime_secs"].as_u64().unwrap_or(0);
    let target = status["target"].as_str().unwrap_or("?");
    let total_players = status["total_players"].as_u64().unwrap_or(0);

    // Fetch journal for per-server breakdown
    let journal = pull_from_hub(&base).await;

    println!();
    println!("┌─ Node: {base} ──────────────────────────────");
    println!("│ Version : {}", status["version"].as_str().unwrap_or("?"));
    println!("│ Target  : {target}");
    println!("│ Uptime  : {}m {}s", uptime / 60, uptime % 60);
    println!("│ Players : {total_players} tracked across all servers");
    println!("├─ Servers ──────────────────────────────────");

    if let Some(ref j) = journal {
        if j.players.is_empty() {
            println!("│ (no servers reporting yet)");
        } else {
            for (srv, entries) in &j.players {
                let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
                println!(
                    "│ {:42} {:>3} players  {}",
                    srv,
                    entries.len(),
                    if names.is_empty() { String::new() } else { format!("[{}]", names.join(", ")) }
                );
            }
        }
    } else {
        println!("│ (journal unavailable)");
    }
    println!("└────────────────────────────────────────────");
}

/// Pull status info from a Hub.
pub async fn pull_status(hub_url: &str) -> Option<serde_json::Value> {
    let url = format!("{}/status", hub_url.trim_end_matches('/'));
    match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                resp.json().await.ok()
            } else {
                None
            }
        }
        Err(e) => {
            log::warn!("[sync] Failed to pull status from hub {}: {}", hub_url, e);
            None
        }
    }
}
