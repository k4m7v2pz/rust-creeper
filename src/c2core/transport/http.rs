//! HTTP(S) 传输实现（占位）
//!
//! TODO: 使用 `reqwest` / `hyper` 实现 HTTP 长轮询或 WebSocket 隧道。
//! 当前仅返回一个"未实现"错误，保证编译通过。

use super::*;

/// HTTP 传输层（暂未实现）。
#[derive(Debug, Clone, Default)]
pub struct HttpTransport;

impl Transport for HttpTransport {
    fn connect(&self, _addr: &str) -> Result<Box<dyn Connection>> {
        Err("HTTP transport is not yet implemented".into())
    }

    fn name(&self) -> &'static str {
        "HTTP"
    }
}
