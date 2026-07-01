//! TCP 传输实现

use super::*;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

// ── Transport ──────────────────────────────────────────────────────

/// TCP 传输层。
#[derive(Debug, Clone, Default)]
pub struct TcpTransport {
    /// 连接超时（默认 10s）
    pub timeout: Duration,
}

impl TcpTransport {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Transport for TcpTransport {
    fn connect(&self, addr: &str) -> Result<Box<dyn Connection>> {
        // 解析地址（取第一个 resolved 地址）
        let socket_addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| format!("unable to resolve address: {addr}"))?;

        let stream = TcpStream::connect_timeout(&socket_addr, self.timeout)?;
        Ok(Box::new(TcpConnection { stream }))
    }

    fn name(&self) -> &'static str {
        "TCP"
    }
}

// ── Connection ─────────────────────────────────────────────────────

/// TCP 连接包装。
#[derive(Debug)]
pub struct TcpConnection {
    stream: TcpStream,
}

impl Read for TcpConnection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for TcpConnection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.stream.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

impl Connection for TcpConnection {
    fn is_alive(&self) -> bool {
        // 简单的存活检测：写入 0 字节测试连接状态
        self.stream.peer_addr().is_ok()
    }

    fn local_addr(&self) -> Result<String> {
        Ok(self.stream.local_addr()?.to_string())
    }

    fn peer_addr(&self) -> Result<String> {
        Ok(self.stream.peer_addr()?.to_string())
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.stream.set_read_timeout(timeout)?)
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.stream.set_write_timeout(timeout)?)
    }
}
