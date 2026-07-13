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
use crate::tasks::{self, Task, TaskQueue, TaskResult, TaskStatus, TaskType};
use crate::host::HostReport;
use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use serde::Serialize;
use std::collections::BTreeMap;
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
    pub target: String,
    pub task_queue: Mutex<TaskQueue>,
    /// 本机资源探针聚合：hostname → 最近一次 HostReport
    pub host_reports: Mutex<BTreeMap<String, HostReport>>,
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
    let pending_tasks = state.task_queue.lock().unwrap().pending_count();
    Utf8Json(serde_json::json!({
        "status": "ok",
        "uptime_secs": uptime,
        "target": state.target,
        "total_players": player_count,
        "pending_tasks": pending_tasks,
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn post_task(
    State(state): State<Arc<HubState>>,
    Json(req): Json<serde_json::Value>,
) -> Utf8Json<serde_json::Value> {
    let domains: Vec<String> = req.get("domains")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    
    let ports: Vec<u16> = req.get("ports")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|u| u as u16)).collect())
        .unwrap_or_default();
    
    let concurrency = req.get("concurrency")
        .and_then(|v| v.as_u64())
        .map(|u| u as usize)
        .unwrap_or(5);

    if domains.is_empty() {
        return Utf8Json(serde_json::json!({
            "error": "domains is required",
            "success": false
        }));
    }

    let task = Task::new(TaskType::Discover, tasks::ScanParams {
        domains,
        ports,
        concurrency,
    });

    state.task_queue.lock().unwrap().enqueue(task.clone());
    
    Utf8Json(serde_json::json!({
        "success": true,
        "task_id": task.id,
        "status": "pending",
        "domains": task.params.domains.len(),
        "ports": task.params.ports.len()
    }))
}

async fn get_tasks(State(state): State<Arc<HubState>>) -> Utf8Json<Vec<Task>> {
    let tasks = state.task_queue.lock().unwrap().list();
    Utf8Json(tasks)
}

async fn get_task(
    State(state): State<Arc<HubState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.task_queue.lock().unwrap().get(&task_id) {
        Some(task) => Utf8Json(task.clone()).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

async fn get_task_dequeue(
    State(state): State<Arc<HubState>>,
    axum::extract::Query(params): axum::extract::Query<serde_json::Value>,
) -> Utf8Json<serde_json::Value> {
    let worker_id = params.get("worker_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    
    match state.task_queue.lock().unwrap().dequeue(worker_id) {
        Some(task) => Utf8Json(serde_json::json!({
            "success": true,
            "task": task
        })),
        None => Utf8Json(serde_json::json!({
            "success": false,
            "message": "no pending tasks"
        })),
    }
}

async fn put_task_result(
    State(state): State<Arc<HubState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Utf8Json<serde_json::Value> {
    let mut queue = state.task_queue.lock().unwrap();
    let mut task = match queue.get(&task_id).cloned() {
        Some(t) => t,
        None => return Utf8Json(serde_json::json!({
            "success": false,
            "error": "task not found"
        })),
    };

    let found_servers = req.get("found_servers")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    
    let discovered = req.get("discovered")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|item| {
            let host = item.get("host").and_then(|h| h.as_str())?;
            let port = item.get("port").and_then(|p| p.as_u64())?;
            Some((host.to_string(), port as u16))
        }).collect())
        .unwrap_or_default();

    let error_str = req.get("error").and_then(|v| v.as_str()).map(String::from);

    if let Some(ref e) = error_str {
        task.fail(e);
    } else {
        task.complete(TaskResult {
            found_servers,
            discovered,
            error: None,
        });
    }

    let task_id = task.id.clone();
    let task_status = format!("{:?}", task.status);
    queue.update(task);
    
    Utf8Json(serde_json::json!({
        "success": true,
        "task_id": task_id,
        "status": task_status
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
            "/":            "this page",
            "/journal":     "GET — full player journal",
            "/merge":       "POST — merge remote journal",
            "/status":      "GET — health & stats",
            "/host-report": "POST — submit host probe; GET — list all probes",
        },
        "players": players,
    }))
}

// -------------------- host probe endpoints --------------------

/// POST /host-report — 接收一次本机资源探针快照，按 hostname 覆盖登记。
async fn post_host_report(
    State(state): State<Arc<HubState>>,
    Json(report): Json<HostReport>,
) -> Utf8Json<serde_json::Value> {
    let hostname = report.hostname.clone();
    let ts = report.timestamp;
    {
        let mut reports = state.host_reports.lock().unwrap();
        reports.insert(hostname.clone(), report);
    }
    log::info!("[hub] host-report stored: {} @ ts={}", hostname, ts);
    Utf8Json(serde_json::json!({
        "ok": true,
        "hostname": hostname,
        "timestamp": ts,
        "stored": true,
    }))
}

/// GET /host-report — 列出所有已登记节点的最近一次快照。
async fn get_host_report(
    State(state): State<Arc<HubState>>,
) -> Utf8Json<serde_json::Value> {
    let reports = state.host_reports.lock().unwrap();
    let nodes: Vec<&HostReport> = reports.values().collect();
    Utf8Json(serde_json::json!({
        "node_count": nodes.len(),
        "nodes": nodes,
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
        task_queue: Mutex::new(TaskQueue::new(tasks::default_queue_path())),
        host_reports: Mutex::new(BTreeMap::new()),
    });

    // 后台定时持久化 journal（每60秒），防止重启丢数据
    let persist_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            persist_state.journal.lock().unwrap().save();
        }
    });

    let app = Router::new()
        .route("/", get(get_root))
        .route("/journal", get(get_journal))
        .route("/merge", post(post_merge))
        .route("/status", get(get_status))
        .route("/tasks", get(get_tasks))
        .route("/tasks", post(post_task))
        .route("/tasks/dequeue", get(get_task_dequeue))
        .route("/tasks/:task_id", get(get_task))
        .route("/tasks/:task_id/result", put(put_task_result))
        .route("/host-report", post(post_host_report))
        .route("/host-report", get(get_host_report))
        .with_state(state.clone());

    let addr = format!("{}:{}", host, port);
    log::info!("[sync] Hub listening on http://{addr}");
    println!("Creeper Hub listening on http://{addr}");
    println!("  GET  /journal       — full player journal");
    println!("  POST /merge         — merge remote journal");
    println!("  GET  /status        — health & stats");
    println!("  POST /tasks         — submit scan task");
    println!("  GET  /tasks         — list all tasks");
    println!("  GET  /tasks/dequeue — claim a task (worker)");
    println!("  POST /host-report   — host probe submit (host-monitor)");
    println!("  GET  /host-report   — list all host probes");

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
    let base = if hub_url.contains("://") {
        hub_url.trim_end_matches('/').to_string()
    } else {
        format!("http://{}", hub_url.trim_end_matches('/'))
    };

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
    let pending_tasks = status["pending_tasks"].as_u64().unwrap_or(0);

    let journal = pull_from_hub(&base).await;
    let tasks = pull_tasks(&base).await;

    let running_tasks = tasks.iter().filter(|t| {
        t["status"].as_str().unwrap_or("") == "Running"
    }).count();

    println!();
    println!("┌─ Node: {base} ──────────────────────────────");
    println!("│ Version : {}", status["version"].as_str().unwrap_or("?"));
    println!("│ Target  : {target}");
    println!("│ Uptime  : {}m {}s", uptime / 60, uptime % 60);
    println!("│ Players : {total_players} tracked across all servers");
    println!("│ Tasks   : {} pending, {} running", pending_tasks, running_tasks);
    println!("├─ Servers ──────────────────────────────────");

    if let Some(ref j) = journal {
        if j.players.is_empty() {
            println!("│ (no servers reporting yet)");
        } else {
            for (srv, entries) in &j.players {
                let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
                let names_str = if names.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", names.join(", "))
                };
                println!(
                    "│ {:<35}  {:>3} players{}",
                    srv,
                    entries.len(),
                    names_str
                );
            }
        }
    } else {
        println!("│ (journal unavailable)");
    }

    if !tasks.is_empty() {
        println!("├─ Tasks ───────────────────────────────────");
        for task in tasks {
            let id = task["id"].as_str().unwrap_or("?");
            let status = task["status"].as_str().unwrap_or("?");
            let domains = task["params"]["domains"].as_array().map(|a| a.len()).unwrap_or(0);
            let ports = task["params"]["ports"].as_array().map(|a| a.len()).unwrap_or(0);
            let claimed = task["claimed_by"].as_str().unwrap_or("-");
            println!("│ {}  {}  {} domains  {} ports  {}",
                id.split('-').next().unwrap_or(id),
                status,
                domains,
                ports,
                if claimed != "-" { format!("({})", claimed.split('-').last().unwrap_or(claimed)) } else { "-".to_string() }
            );
        }
    }

    println!("└────────────────────────────────────────────");
}

async fn pull_tasks(hub_url: &str) -> Vec<serde_json::Value> {
    let url = format!("{}/tasks", hub_url.trim_end_matches('/'));
    match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                resp.json().await.ok().unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    }
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
