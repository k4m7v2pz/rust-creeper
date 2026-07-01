//! UDP 传输实现
//!
//! 由于 UDP 是无连接的，`connect()` 会绑定本地端口并将 socket "连接" 到目标地址
//! （仅内核记录对端地址，后续 `send`/`recv` 可用）。

use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

// ── Transport ──────────────────────────────────────────────────────

/// UDP 传输层。
#[derive(Debug, Clone)]
pub struct UdpTransport {
    /// 本地绑定地址（默认 `0.0.0.0:0`）
    pub bind_addr: String,
    /// 读写超时
    pub timeout: Duration,
}

impl Default for UdpTransport {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:0".into(),
            timeout: Duration::from_secs(10),
        }
    }
}

impl Transport for UdpTransport {
    fn connect(&self, addr: &str) -> Result<Box<dyn Connection>> {
        let socket = UdpSocket::bind(&self.bind_addr)?;
        socket.set_read_timeout(Some(self.timeout))?;
        socket.set_write_timeout(Some(self.timeout))?;

        // "connect" UDP socket 到目标地址
        let target: SocketAddr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| format!("unable to resolve address: {addr}"))?;

        socket.connect(target)?;

        Ok(Box::new(UdpConnection { socket }))
    }

    fn name(&self) -> &'static str {
        "UDP"
    }
}

// ── Connection ─────────────────────────────────────────────────────

/// UDP 连接包装。
///
/// 注意：`Read` 实现使用 `recv`，`Write` 实现使用 `send`。
/// 由于 UDP 可能丢包/乱序，上层需自行处理可靠性和分片。
#[derive(Debug)]
pub struct UdpConnection {
    socket: UdpSocket,
}

impl Read for UdpConnection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.socket.recv(buf)
    }
}

impl Write for UdpConnection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.socket.send(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        // UDP 无缓冲，flush 为空操作
        Ok(())
    }
}

impl Connection for UdpConnection {
    fn is_alive(&self) -> bool {
        // UDP 无真实连接状态，只能判断 socket 是否有效
        self.socket.local_addr().is_ok()
    }

    fn local_addr(&self) -> Result<String> {
        Ok(self.socket.local_addr()?.to_string())
    }

    fn peer_addr(&self) -> Result<String> {
        Ok(self.socket.peer_addr()?.to_string())
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.socket.set_read_timeout(timeout)?)
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.socket.set_write_timeout(timeout)?)
    }
}
