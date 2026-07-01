//! # 传输层抽象
//!
//! 定义 `Transport` 和 `Connection` 两个核心 trait，
//! 将底层网络协议（TCP / UDP / HTTP / …）统一到同一接口下。

pub mod tcp;
pub mod udp;
pub mod r#http;

use std::fmt::Debug;
use std::io::{Read, Write};
use std::time::Duration;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ── Connection trait ───────────────────────────────────────────────

/// 一条已建立的连接（半双工读写 + 元信息）。
///
/// 实现了 `Read + Write`，上层可以直接用 `std::io` 读写。
pub trait Connection: Read + Write + Send + Debug {
    /// 连接是否仍然存活
    fn is_alive(&self) -> bool;

    /// 本端地址
    fn local_addr(&self) -> Result<String>;

    /// 对端地址
    fn peer_addr(&self) -> Result<String>;

    /// 设置读超时
    fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()>;

    /// 设置写超时
    fn set_write_timeout(&self, timeout: Option<Duration>) -> Result<()>;
}

// ── Transport trait ────────────────────────────────────────────────

/// 传输层协议实现。
///
/// 每种协议（TCP、UDP、HTTP 等）实现此 trait，
/// 调用方无需关心底层是流式还是数据包式传输。
pub trait Transport: Send + Sync + Debug {
    /// 连接到目标地址（格式由具体实现定义，例如 `host:port`）
    fn connect(&self, addr: &str) -> Result<Box<dyn Connection>>;

    /// 传输层名称（用于日志 / 显示）
    fn name(&self) -> &'static str;
}
