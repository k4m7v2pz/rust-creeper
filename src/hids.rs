//! HIDS — Host-based Intrusion Detection Self-check
//!
//! 本模块是**只读**的主机入侵指标自检工具，用于在自己管理的机器上
//! 检查是否存在被入侵 / 被持久化 / 凭据泄漏的痕迹。
//!
//! - 不修改任何文件
//! - 不连接网络
//! - 不执行破坏性操作
//!
//! 设计目标平台：Linux（Windows Server 部分检查在非 Windows 系统上跳过）。

use std::fs;
use std::path::Path;

use serde::Serialize;

// ---------------------------------------------------------------------------
// 公共类型
// ---------------------------------------------------------------------------

/// 单条检查结果
#[derive(Debug, Clone, Serialize)]
pub struct CheckItem {
    /// 检查类别：ssh_keys / shell_history / cron / systemd / shm_exec / cloud_creds / weak_pass / startup
    pub category: &'static str,
    /// 检查项名称
    pub name: String,
    /// 严重程度：ok / warn / danger
    pub level: &'static str,
    /// 详细描述
    pub message: String,
    /// 涉及的路径（如果有）
    pub path: Option<String>,
    /// 建议操作
    pub suggestion: Option<String>,
}

/// HIDS 检查报告
#[derive(Debug, Clone, Serialize)]
pub struct HidsReport {
    /// 主机信息
    pub hostname: String,
    /// 检查时间戳（Unix 秒）
    pub timestamp: u64,
    /// 全部检查项
    pub checks: Vec<CheckItem>,
    /// 汇总
    pub summary: CheckSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckSummary {
    pub total: usize,
    pub ok: usize,
    pub warn: usize,
    pub danger: usize,
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 执行全部 HIDS 检查，返回报告
pub fn run_all() -> HidsReport {
    let mut checks: Vec<CheckItem> = Vec::new();

    // 1. SSH authorized_keys
    checks.extend(check_ssh_authorized_keys());

    // 2. Shell history
    checks.extend(check_shell_history());

    // 3. Systemd services
    checks.extend(check_systemd_services());

    // 4. Cron jobs
    checks.extend(check_cron_jobs());

    // 5. /dev/shm executables
    checks.extend(check_shm_executables());

    // 6. Cloud credentials
    checks.extend(check_cloud_credentials());

    // 7. Startup scripts (bashrc, profile)
    checks.extend(check_startup_scripts());

    // 8. Weak password self-check (shadow)
    checks.extend(check_weak_passwords());

    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".into());

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let ok = checks.iter().filter(|c| c.level == "ok").count();
    let warn = checks.iter().filter(|c| c.level == "warn").count();
    let danger = checks.iter().filter(|c| c.level == "danger").count();

    HidsReport {
        hostname,
        timestamp,
        checks,
        summary: CheckSummary {
            total: ok + warn + danger,
            ok,
            warn,
            danger,
        },
    }
}

// ---------------------------------------------------------------------------
// 1. SSH authorized_keys 检查
// ---------------------------------------------------------------------------

fn check_ssh_authorized_keys() -> Vec<CheckItem> {
    let mut results = Vec::new();

    // Linux 常规路径
    let paths = [
        // 当前用户
        expand_tilde("~/.ssh/authorized_keys"),
        // root 用户
        "/root/.ssh/authorized_keys".into(),
        // Windows OpenSSH
        expand_tilde("~/.ssh/authorized_keys"),
    ];

    let mut seen = std::collections::HashSet::new();
    for path in &paths {
        if !seen.insert(path.clone()) {
            continue;
        }
        let p = Path::new(path);
        if !p.exists() {
            continue;
        }

        match fs::read_to_string(p) {
            Ok(content) => {
                let keys: Vec<&str> = content.lines().filter(|l| {
                    let l = l.trim();
                    !l.is_empty() && !l.starts_with('#')
                }).collect();

                if keys.is_empty() {
                    results.push(CheckItem {
                        category: "ssh_keys",
                        name: format!("{} 为空", path),
                        level: "warn",
                        message: format!("{} 存在但内容为空——可能被清空过", path),
                        path: Some(path.clone()),
                        suggestion: Some("检查是否被入侵者清空后追加了自己的 key".into()),
                    });
                } else {
                    // 检查是否有过多 key（异常指标）
                    if keys.len() > 5 {
                        results.push(CheckItem {
                            category: "ssh_keys",
                            name: format!("{} 有 {} 个 key", path, keys.len()),
                            level: "warn",
                            message: format!("{} 包含 {} 个公钥，数量偏多", path, keys.len()),
                            path: Some(path.clone()),
                            suggestion: Some("逐一检查每个 key 的来源是否合法".into()),
                        });
                    } else {
                        for (i, key) in keys.iter().enumerate() {
                            let preview = if key.len() > 60 {
                                format!("{}...", &key[..60])
                            } else {
                                key.to_string()
                            };
                            results.push(CheckItem {
                                category: "ssh_keys",
                                name: format!("SSH key #{}", i + 1),
                                level: "ok",
                                message: format!("{}: {}", path, preview),
                                path: Some(path.clone()),
                                suggestion: None,
                            });
                        }
                    }
                }
            }
            Err(e) => {
                results.push(CheckItem {
                    category: "ssh_keys",
                    name: format!("{} 不可读", path),
                    level: "warn",
                    message: format!("无法读取 {}: {}", path, e),
                    path: Some(path.clone()),
                    suggestion: Some("检查权限是否被篡改".into()),
                });
            }
        }
    }

    if results.is_empty() {
        results.push(CheckItem {
            category: "ssh_keys",
            name: "未找到 SSH authorized_keys".into(),
            level: "ok",
            message: "未发现 SSH authorized_keys 文件（可能未配置 SSH 公钥登录）".into(),
            path: None,
            suggestion: None,
        });
    }

    results
}

// ---------------------------------------------------------------------------
// 2. Shell history 检查
// ---------------------------------------------------------------------------

/// 可疑的 history 清理命令
const SUSPICIOUS_HISTORY_COMMANDS: &[&str] = &[
    "history -c",
    "history -w",
    "shred ~/.bash_history",
    "rm ~/.bash_history",
    "rm -f ~/.bash_history",
    "> ~/.bash_history",
    "cat /dev/null > ~/.bash_history",
    "echo '' > ~/.bash_history",
    "shred -u ~/.bash_history",
    "unset HISTORY",
    "export HISTFILE=/dev/null",
    "HISTFILE=/dev/null",
    "set +o history",
    "shred ~/.zsh_history",
    "rm ~/.zsh_history",
    "rm -f ~/.zsh_history",
];

fn check_shell_history() -> Vec<CheckItem> {
    let mut results = Vec::new();

    let history_paths = [
        ("bash", expand_tilde("~/.bash_history")),
        ("zsh",  expand_tilde("~/.zsh_history")),
        ("python", expand_tilde("~/.python_history")),
        ("mysql", expand_tilde("~/.mysql_history")),
        ("psql",  expand_tilde("~/.psql_history")),
    ];

    for (shell, path) in &history_paths {
        let p = Path::new(path);
        if !p.exists() {
            results.push(CheckItem {
                category: "shell_history",
                name: format!("{} history 不存在", shell),
                level: "warn",
                message: format!("{} 的 history 文件不存在（{}）——可能被删除了", shell, path),
                path: Some(path.clone()),
                suggestion: Some("检查是否被入侵者清理了操作痕迹".into()),
            });
            continue;
        }

        match fs::read_to_string(p) {
            Ok(content) => {
                if content.trim().is_empty() {
                    results.push(CheckItem {
                        category: "shell_history",
                        name: format!("{} history 为空", shell),
                        level: "warn",
                        message: format!("{} 的 history 文件为空（{}）", shell, path),
                        path: Some(path.clone()),
                        suggestion: Some("history 被清空可能是入侵者清理痕迹的迹象".into()),
                    });
                    continue;
                }

                let lines: Vec<&str> = content.lines().collect();
                // 检查是否有可疑清理命令
                let suspicious: Vec<&str> = lines.iter()
                    .filter(|l| SUSPICIOUS_HISTORY_COMMANDS.iter().any(|c| l.contains(c)))
                    .copied()
                    .collect();

                if !suspicious.is_empty() {
                    results.push(CheckItem {
                        category: "shell_history",
                        name: format!("{} history 含可疑命令", shell),
                        level: "danger",
                        message: format!("{} history 中发现 {} 条可疑清理/隐匿命令", shell, suspicious.len()),
                        path: Some(path.clone()),
                        suggestion: Some("检查这些命令的执行上下文，可能是入侵者试图掩盖痕迹".into()),
                    });
                } else {
                    results.push(CheckItem {
                        category: "shell_history",
                        name: format!("{} history 正常", shell),
                        level: "ok",
                        message: format!("{} history 有 {} 条记录，未发现可疑清理命令", shell, lines.len()),
                        path: Some(path.clone()),
                        suggestion: None,
                    });
                }
            }
            Err(e) => {
                results.push(CheckItem {
                    category: "shell_history",
                    name: format!("{} history 不可读", shell),
                    level: "warn",
                    message: format!("无法读取 {}: {}", path, e),
                    path: Some(path.clone()),
                    suggestion: Some("检查文件权限".into()),
                });
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// 3. Systemd services 检查
// ---------------------------------------------------------------------------

/// 已知的可疑服务名关键词
const SUSPICIOUS_SERVICE_KEYWORDS: &[&str] = &[
    "crypto", "miner", "xmr", "monero", "kworker", "cron", "syslogd",
    "trojan", "backdoor", "shell", "rev",
];

fn check_systemd_services() -> Vec<CheckItem> {
    let mut results = Vec::new();

    // 只在 Linux 上执行
    if cfg!(not(target_os = "linux")) {
        results.push(CheckItem {
            category: "systemd",
            name: "systemd 检查（非 Linux 跳过）".into(),
            level: "ok",
            message: "当前系统不是 Linux，跳过 systemd 检查".into(),
            path: None,
            suggestion: None,
        });
        return results;
    }

    // 检查 systemd 服务目录是否存在
    let systemd_dirs = [
        "/etc/systemd/system/",
        "/usr/lib/systemd/system/",
        "/lib/systemd/system/",
    ];

    let mut all_units: Vec<String> = Vec::new();
    for dir in &systemd_dirs {
        let p = Path::new(dir);
        if !p.is_dir() {
            continue;
        }
        match fs::read_dir(p) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".service") {
                            all_units.push(format!("{}{}", dir, name));
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }

    if all_units.is_empty() {
        results.push(CheckItem {
            category: "systemd",
            name: "systemd 服务检查".into(),
            level: "ok",
            message: "未找到 systemd 服务文件（可能没有 systemd）".into(),
            path: None,
            suggestion: None,
        });
        return results;
    }

    results.push(CheckItem {
        category: "systemd",
        name: format!("systemd 服务总数: {}", all_units.len()),
        level: "ok",
        message: format!("共发现 {} 个 systemd service 文件", all_units.len()),
        path: None,
        suggestion: None,
    });

    // 检查可疑服务
    for unit_path in &all_units {
        let name = Path::new(unit_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_lowercase();

        let matched: Vec<&str> = SUSPICIOUS_SERVICE_KEYWORDS
            .iter()
            .filter(|kw| name.contains(*kw))
            .copied()
            .collect();

        if !matched.is_empty() {
            // 读取服务文件内容查看 ExecStart
            let exec_line = fs::read_to_string(unit_path)
                .ok()
                .and_then(|c| {
                    c.lines().find(|l| {
                        let t = l.trim();
                        t.starts_with("ExecStart=") || t.starts_with("ExecStartPre=")
                    }).map(|l| l.to_string())
                })
                .unwrap_or_else(|| "(unreadable)".into());

            results.push(CheckItem {
                category: "systemd",
                name: format!("可疑服务: {}", name),
                level: "warn",
                message: format!("服务名包含可疑关键词 {:?}\n  ExecStart: {}", matched, exec_line),
                path: Some(unit_path.clone()),
                suggestion: Some("检查该服务是否合法，非预期服务可能是持久化后门".into()),
            });
        }
    }

    // 检查最近新增的服务（通过修改时间）
    let now = std::time::SystemTime::now();
    let one_week = std::time::Duration::from_secs(7 * 24 * 3600);
    for unit_path in &all_units {
        let p = Path::new(unit_path);
        if let Ok(meta) = p.metadata() {
            if let Ok(modified) = meta.modified() {
                if let Ok(duration) = now.duration_since(modified) {
                    if duration < one_week {
                        let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                        results.push(CheckItem {
                            category: "systemd",
                            name: format!("近期新增服务: {}", name),
                            level: "warn",
                            message: format!("{} 是在最近一周内创建的", unit_path),
                            path: Some(unit_path.clone()),
                            suggestion: Some("检查近期新增的服务是否合法".into()),
                        });
                    }
                }
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// 4. Cron jobs 检查
// ---------------------------------------------------------------------------

fn check_cron_jobs() -> Vec<CheckItem> {
    let mut results = Vec::new();

    if cfg!(not(target_os = "linux")) {
        results.push(CheckItem {
            category: "cron",
            name: "cron 检查（非 Linux 跳过）".into(),
            level: "ok",
            message: "当前系统不是 Linux，跳过 cron 检查".into(),
            path: None,
            suggestion: None,
        });
        return results;
    }

    // 检查 crontab 文件
    let cron_paths = [
        "/etc/crontab",
        "/etc/cron.d/",
        "/var/spool/cron/crontabs/",
        "/var/spool/cron/",
    ];

    for path in &cron_paths {
        let p = Path::new(path);
        if !p.exists() {
            continue;
        }

        if p.is_dir() {
            match fs::read_dir(p) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        let fpath = format!("{}{}", path, fname);
                        let content = fs::read_to_string(&fpath).unwrap_or_default();
                        let lines: Vec<&str> = content.lines()
                            .filter(|l| {
                                let t = l.trim();
                                !t.is_empty() && !t.starts_with('#')
                            })
                            .collect();

                        if !lines.is_empty() {
                            results.push(CheckItem {
                                category: "cron",
                                name: format!("cron 文件: {}", fname),
                                level: "ok",
                                message: format!("{} 有 {} 条定时任务", fpath, lines.len()),
                                path: Some(fpath),
                                suggestion: None,
                            });
                        }
                    }
                }
                Err(_) => {}
            }
        } else if p.is_file() {
            let content = fs::read_to_string(path).unwrap_or_default();
            let lines: Vec<&str> = content.lines()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty() && !t.starts_with('#')
                })
                .collect();
            if !lines.is_empty() {
                results.push(CheckItem {
                    category: "cron",
                    name: format!("cron 文件: {}", path),
                    level: "ok",
                    message: format!("{} 有 {} 条定时任务", path, lines.len()),
                    path: Some(path.to_string()),
                    suggestion: None,
                });
            }
        }
    }

    if results.is_empty() {
        results.push(CheckItem {
            category: "cron",
            name: "cron 检查".into(),
            level: "ok",
            message: "未发现 crontab 文件（或权限不足）".into(),
            path: None,
            suggestion: None,
        });
    }

    results
}

// ---------------------------------------------------------------------------
// 5. /dev/shm 可执行文件检查
// ---------------------------------------------------------------------------

fn check_shm_executables() -> Vec<CheckItem> {
    let mut results = Vec::new();

    if cfg!(not(target_os = "linux")) {
        results.push(CheckItem {
            category: "shm_exec",
            name: "/dev/shm 检查（非 Linux 跳过）".into(),
            level: "ok",
            message: "当前系统不是 Linux，跳过 /dev/shm 检查".into(),
            path: None,
            suggestion: None,
        });
        return results;
    }

    let shm_path = Path::new("/dev/shm");
    if !shm_path.is_dir() {
        results.push(CheckItem {
            category: "shm_exec",
            name: "/dev/shm 不存在".into(),
            level: "ok",
            message: "/dev/shm 不存在（可能未挂载 tmpfs）".into(),
            path: None,
            suggestion: None,
        });
        return results;
    }

    match fs::read_dir(shm_path) {
        Ok(entries) => {
            let entries: Vec<_> = entries.flatten().collect();
            if entries.is_empty() {
                results.push(CheckItem {
                    category: "shm_exec",
                    name: "/dev/shm 为空".into(),
                    level: "ok",
                    message: "/dev/shm 中没有任何文件".into(),
                    path: None,
                    suggestion: None,
                });
                return results;
            }

            for entry in &entries {
                let fname = entry.file_name().to_string_lossy().to_string();
                let fpath = entry.path();

                if let Ok(meta) = entry.metadata() {
                    let is_exec = meta.is_file() && {
                        // 在 Unix 上检查可执行权限
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            meta.permissions().mode() & 0o111 != 0
                        }
                        #[cfg(not(unix))]
                        {
                            false
                        }
                    };

                    if is_exec {
                        results.push(CheckItem {
                            category: "shm_exec",
                            name: format!("/dev/shm 可执行文件: {}", fname),
                            level: "danger",
                            message: format!("/dev/shm 中存在可执行文件: {} ({:?})", fname, fpath),
                            path: Some(fpath.to_string_lossy().to_string()),
                            suggestion: Some("内存盘中的可执行文件可能是恶意软件——正常情况 /dev/shm 不应有可执行文件".into()),
                        });
                    }
                }
            }

            // 如果没有发现可执行文件
            if !results.iter().any(|r| r.category == "shm_exec" && r.level == "danger") {
                results.push(CheckItem {
                    category: "shm_exec",
                    name: format!("/dev/shm 有 {} 个文件，无可执行文件", entries.len()),
                    level: "ok",
                    message: format!("/dev/shm 中共 {} 个文件，未发现可执行文件", entries.len()),
                    path: None,
                    suggestion: None,
                });
            }
        }
        Err(e) => {
            results.push(CheckItem {
                category: "shm_exec",
                name: "/dev/shm 不可读".into(),
                level: "warn",
                message: format!("无法读取 /dev/shm: {}", e),
                path: None,
                suggestion: Some("检查权限".into()),
            });
        }
    }

    results
}

// ---------------------------------------------------------------------------
// 6. 云凭据泄漏检查
// ---------------------------------------------------------------------------

const CLOUD_CRED_PATHS: &[(&str, &str)] = &[
    ("AWS",       "~/.aws/credentials"),
    ("AWS config","~/.aws/config"),
    ("GCloud",    "~/.config/gcloud/application_default_credentials.json"),
    ("Azure",     "~/.azure/azureProfile.json"),
    ("Docker",    "~/.docker/config.json"),
    ("Kube",      "~/.kube/config"),
    ("Kube (old)","~/.kube/kubeconfig"),
];

fn check_cloud_credentials() -> Vec<CheckItem> {
    let mut results = Vec::new();

    for (provider, rel_path) in CLOUD_CRED_PATHS {
        let path = expand_tilde(rel_path);
        let p = Path::new(&path);
        if !p.exists() {
            continue;
        }

        match fs::read_to_string(p) {
            Ok(content) => {
                let has_secret = content.contains("secret")
                    || content.contains("token")
                    || content.contains("password")
                    || content.contains("key");
                let line_count = content.lines().count();

                if has_secret {
                    results.push(CheckItem {
                        category: "cloud_creds",
                        name: format!("{} 凭据文件", provider),
                        level: "warn",
                        message: format!("{} 的凭据文件存在且包含敏感信息（{} 行）: {}",
                            provider, line_count, path),
                        path: Some(path),
                        suggestion: Some("检查该凭据是否仍在使用中，未使用的凭据应及时清理".into()),
                    });
                } else {
                    results.push(CheckItem {
                        category: "cloud_creds",
                        name: format!("{} 配置文件", provider),
                        level: "ok",
                        message: format!("{} 配置文件存在但未发现明文密钥: {}", provider, path),
                        path: Some(path),
                        suggestion: None,
                    });
                }
            }
            Err(_) => {
                // 文件存在但不可读（可能是权限问题，正常现象）
            }
        }
    }

    if results.is_empty() {
        results.push(CheckItem {
            category: "cloud_creds",
            name: "云凭据检查".into(),
            level: "ok",
            message: "未发现云服务凭据文件".into(),
            path: None,
            suggestion: None,
        });
    }

    results
}

// ---------------------------------------------------------------------------
// 7. 启动脚本检查（bashrc / profile）
// ---------------------------------------------------------------------------

const STARTUP_SCRIPT_PATHS: &[&str] = &[
    "~/.bashrc",
    "~/.bash_profile",
    "~/.profile",
    "~/.zshrc",
    "~/.zshenv",
    "~/.config/fish/config.fish",
    "/etc/profile",
    "/etc/profile.d/",
    "/etc/bash.bashrc",
    "/etc/zsh/zshrc",
];

/// 可疑的启动脚本关键词
const SUSPICIOUS_STARTUP_KEYWORDS: &[&str] = &[
    "curl", "wget", "nc -e", "bash -i", "sh -i",
    "perl -e", "python -c", "ncat",
    "chmod +x", "chmod 777",
    ">/dev/tcp/", ">/dev/udp/",
    "mkfifo", "mknod",
    "./", "/tmp/", "/dev/shm/",
    "crontab", "at now",
];

fn check_startup_scripts() -> Vec<CheckItem> {
    let mut results = Vec::new();

    for rel_path in STARTUP_SCRIPT_PATHS {
        let path = expand_tilde(rel_path);
        let p = Path::new(&path);

        if !p.exists() {
            continue;
        }

        if p.is_dir() {
            // 检查 /etc/profile.d/
            match fs::read_dir(p) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        let fpath = entry.path().to_string_lossy().to_string();
                        let content = fs::read_to_string(&fpath).unwrap_or_default();
                        let suspicious: Vec<&str> = SUSPICIOUS_STARTUP_KEYWORDS
                            .iter()
                            .filter(|kw| content.contains(*kw))
                            .copied()
                            .collect();

                        if !suspicious.is_empty() {
                            results.push(CheckItem {
                                category: "startup",
                                name: format!("可疑启动脚本: {}", fname),
                                level: "danger",
                                message: format!("{} 包含可疑命令 {:?}", fpath, suspicious),
                                path: Some(fpath),
                                suggestion: Some("启动脚本中的远程下载/反向 shell 命令可能是持久化后门".into()),
                            });
                        }
                    }
                }
                Err(_) => {}
            }
            continue;
        }

        match fs::read_to_string(p) {
            Ok(content) => {
                let suspicious: Vec<&str> = SUSPICIOUS_STARTUP_KEYWORDS
                    .iter()
                    .filter(|kw| content.contains(*kw))
                    .copied()
                    .collect();

                if !suspicious.is_empty() {
                    results.push(CheckItem {
                        category: "startup",
                        name: format!("可疑启动脚本: {}", rel_path),
                        level: "danger",
                        message: format!("{} 包含可疑命令 {:?}", path, suspicious),
                        path: Some(path),
                        suggestion: Some("启动脚本中的远程下载/反向 shell 命令可能是持久化后门".into()),
                    });
                } else {
                    results.push(CheckItem {
                        category: "startup",
                        name: format!("启动脚本正常: {}", rel_path),
                        level: "ok",
                        message: format!("{} 存在但未发现可疑内容", path),
                        path: Some(path),
                        suggestion: None,
                    });
                }
            }
            Err(e) => {
                results.push(CheckItem {
                    category: "startup",
                    name: format!("启动脚本不可读: {}", rel_path),
                    level: "warn",
                    message: format!("无法读取 {}: {}", path, e),
                    path: Some(path),
                    suggestion: Some("检查文件权限".into()),
                });
            }
        }
    }

    if results.is_empty() {
        results.push(CheckItem {
            category: "startup",
            name: "启动脚本检查".into(),
            level: "ok",
            message: "未发现启动脚本文件".into(),
            path: None,
            suggestion: None,
        });
    }

    results
}

// ---------------------------------------------------------------------------
// 8. 弱口令自检（Linux /etc/shadow）
// ---------------------------------------------------------------------------

/// 常见弱口令哈希前缀表（仅用于警示，不做实际破解）
const WEAK_PASSWORD_HINTS: &[&str] = &[
    "Password111",
    "Password222",
    "Password333",
    "Password444",
    "Password555",
    "Password666",
    "Password777",
    "Password888",
    "Password999",
    "Password01",
    "Password02",
    "Password12",
    "Password123",
    "admin123",
    "root123",
    "123456",
    "password",
    "P@ssw0rd",
    "letmein",
];

fn check_weak_passwords() -> Vec<CheckItem> {
    let mut results = Vec::new();

    if cfg!(not(target_os = "linux")) {
        results.push(CheckItem {
            category: "weak_pass",
            name: "弱口令检查（非 Linux 跳过）".into(),
            level: "ok",
            message: "当前系统不是 Linux，跳过 /etc/shadow 弱口令检查".into(),
            path: None,
            suggestion: None,
        });
        return results;
    }

    let shadow_path = Path::new("/etc/shadow");
    if !shadow_path.exists() {
        results.push(CheckItem {
            category: "weak_pass",
            name: "/etc/shadow 不存在".into(),
            level: "ok",
            message: "/etc/shadow 不存在（可能不是 Linux 或有其他认证机制）".into(),
            path: None,
            suggestion: None,
        });
        return results;
    }

    // 读取 /etc/passwd 获取用户名列表
    let users = fs::read_to_string("/etc/passwd")
        .ok()
        .map(|c| {
            c.lines()
                .filter_map(|l| l.split(':').next())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // 读取 shadow
    match fs::read_to_string(shadow_path) {
        Ok(content) => {
            let mut has_hash = false;
            let mut hash_count = 0;
            for line in content.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() < 2 {
                    continue;
                }
                let user = parts[0];
                let hash = parts[1];

                // 哈希字段非空且不是 ! / * / x 等禁用标记
                if !hash.is_empty() && hash != "!" && hash != "*" && hash != "x" {
                    has_hash = true;
                    hash_count += 1;

                    // 检查是否为空口令
                    if hash.is_empty() || hash == "" {
                        results.push(CheckItem {
                            category: "weak_pass",
                            name: format!("空口令用户: {}", user),
                            level: "danger",
                            message: format!("用户 '{}' 的口令字段为空——该用户不需要密码即可登录", user),
                            path: None,
                            suggestion: Some("立即用 passwd 命令设置密码".into()),
                        });
                    }
                }
            }

            results.push(CheckItem {
                category: "weak_pass",
                name: format!("shadow 中有 {} 个用户", users.len()),
                level: "ok",
                message: format!("/etc/shadow 中共 {} 个用户条目，{} 个有密码哈希", users.len(), hash_count),
                path: None,
                suggestion: None,
            });

            if !has_hash {
                results.push(CheckItem {
                    category: "weak_pass",
                    name: "shadow 无密码哈希".into(),
                    level: "warn",
                    message: "/etc/shadow 中没有任何密码哈希（可能所有用户都使用 SSH 密钥或其他认证方式）".into(),
                    path: None,
                    suggestion: None,
                });
            }
        }
        Err(_e) => {
            results.push(CheckItem {
                category: "weak_pass",
                name: "/etc/shadow 不可读".into(),
                level: "ok",
                message: "无法读取 /etc/shadow（需要 root 权限）——这通常不是问题".into(),
                path: None,
                suggestion: None,
            });
        }
    }

    results
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 扩展 ~ 为 $HOME
fn expand_tilde(path: &str) -> String {
    if !path.starts_with('~') {
        return path.to_string();
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/root".into());
    path.replacen('~', &home, 1)
}

// ---------------------------------------------------------------------------
// 输出格式化
// ---------------------------------------------------------------------------

/// 打印人类可读的报告
pub fn print_report(report: &HidsReport) {
    println!("┌─ HIDS Self-Check ──────────────────────────────────────────");
    println!("│ Hostname : {}", report.hostname);
    println!("│ Time     : {}", chrono::DateTime::from_timestamp(report.timestamp as i64, 0)
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "?".into()));
    println!("│ Scope    : all");
    println!("├─────────────────────────────────────────────────────────────");
    println!();
    println!("  Summary: {} checks | {} ✅ OK | {} ⚠️ WARN | {} 🔴 DANGER",
        report.summary.total, report.summary.ok, report.summary.warn, report.summary.danger);
    println!();

    // 按严重程度分组输出
    let dangers: Vec<&CheckItem> = report.checks.iter().filter(|c| c.level == "danger").collect();
    let warns: Vec<&CheckItem> = report.checks.iter().filter(|c| c.level == "warn").collect();
    let oks: Vec<&CheckItem> = report.checks.iter().filter(|c| c.level == "ok").collect();

    if !dangers.is_empty() {
        println!("  🔴 DANGER ({})", dangers.len());
        println!("  ────────────────────────────────────────────────────────");
        for item in &dangers {
            println!("  [{}] {}", item.category, item.name);
            println!("       {}", item.message);
            if let Some(ref sug) = item.suggestion {
                println!("       💡 {}", sug);
            }
            if let Some(ref p) = item.path {
                println!("       📁 {}", p);
            }
            println!();
        }
    }

    if !warns.is_empty() {
        println!("  ⚠️ WARN ({})", warns.len());
        println!("  ────────────────────────────────────────────────────────");
        for item in &warns {
            println!("  [{}] {}", item.category, item.name);
            println!("       {}", item.message);
            if let Some(ref sug) = item.suggestion {
                println!("       💡 {}", sug);
            }
            if let Some(ref p) = item.path {
                println!("       📁 {}", p);
            }
            println!();
        }
    }

    if !oks.is_empty() {
        println!("  ✅ OK ({})", oks.len());
        println!("  ────────────────────────────────────────────────────────");
        for item in oks.iter().take(10) {
            println!("  [{}] {}", item.category, item.name);
        }
        if oks.len() > 10 {
            println!("  ... and {} more OK items", oks.len() - 10);
        }
        println!();
    }

    println!("└─────────────────────────────────────────────────────────────");
}