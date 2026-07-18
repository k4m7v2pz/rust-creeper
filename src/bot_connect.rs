//! Minimal Minecraft offline-mode bot login using raw TCP.
//!
//! Implements just enough of the Minecraft protocol (Handshake → Login)
//! to connect to a cracked/offline server.  Derived from azalea-protocol
//! packet definitions for protocol 775 (Minecraft 26.1.2).
//!
//! No heavy dependencies — just tokio + anyhow + uuid.

use std::io::{Cursor, Read};

use anyhow::{Context, anyhow, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// VarInt / VarLong helpers
// ---------------------------------------------------------------------------

const SEGMENT_BITS: u32 = 0x7F;
const CONTINUE_BIT: u32 = 0x80;

pub fn write_varint(buf: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    loop {
        if (value & !SEGMENT_BITS) == 0 {
            buf.push(value as u8);
            return;
        }
        buf.push((value & SEGMENT_BITS | CONTINUE_BIT) as u8);
        value >>= 7;
    }
}

pub fn read_varint(buf: &mut Cursor<&[u8]>) -> Result<i32> {
    let mut value: u32 = 0;
    let mut position = 0;
    loop {
        let byte = read_u8(buf)?;
        value |= ((byte & SEGMENT_BITS as u8) as u32) << position;
        if (byte & CONTINUE_BIT as u8) == 0 {
            return Ok(value as i32);
        }
        position += 7;
        if position >= 32 {
            anyhow::bail!("VarInt too big");
        }
    }
}

fn read_u8(buf: &mut Cursor<&[u8]>) -> Result<u8> {
    let mut byte = [0u8; 1];
    Read::read_exact(buf, &mut byte).context("read_u8")?;
    Ok(byte[0])
}

// ---------------------------------------------------------------------------
// Packet writing helpers
// ---------------------------------------------------------------------------

pub fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_varint(buf, bytes.len() as i32);
    buf.extend_from_slice(bytes);
}

pub fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}

pub fn write_uuid(buf: &mut Vec<u8>, uuid: &Uuid) {
    buf.extend_from_slice(uuid.as_bytes());
}

/// Wrap a raw packet payload with its length prefix (VarInt).
pub fn packet_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 5);
    write_varint(&mut frame, payload.len() as i32);
    frame.extend_from_slice(payload);
    frame
}

// ---------------------------------------------------------------------------
// Async read helpers
// ---------------------------------------------------------------------------

/// Read a full Minecraft packet: VarInt length → payload
pub async fn read_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let length = read_varint_async(stream).await?;
    let mut payload = vec![0u8; length as usize];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| anyhow!("read packet payload: {e}"))?;
    Ok(payload)
}

pub async fn read_varint_async(stream: &mut TcpStream) -> Result<i32> {
    let mut value: u32 = 0;
    let mut position = 0;
    loop {
        let mut byte = [0u8; 1];
        stream
            .read_exact(&mut byte)
            .await
            .map_err(|e| anyhow!("read varint: {e}"))?;
        value |= ((byte[0] & SEGMENT_BITS as u8) as u32) << position;
        if (byte[0] & CONTINUE_BIT as u8) == 0 {
            return Ok(value as i32);
        }
        position += 7;
        if position >= 32 {
            anyhow::bail!("VarInt too big");
        }
    }
}

/// Read a VarInt from a byte buffer (synchronous, from Cursor).

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Minecraft account authentication method.
#[derive(Debug, Clone)]
pub enum Account {
    /// Offline mode — no authentication, any username works.
    Offline { username: String },
    /// Online mode — requires Microsoft/Mojang authentication.
    /// **Not yet implemented.** Will eventually need email/password or token.
    Online {
        username: String,
        /// Placeholder for future auth token / password / refresh_token
        _credential: String,
    },
}

impl Account {
    pub fn username(&self) -> &str {
        match self {
            Self::Offline { username } => username,
            Self::Online { username, .. } => username,
        }
    }

    /// Returns true for offline-mode accounts.
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline { .. })
    }
}

/// Connection result from a bot login attempt.
#[derive(Debug)]
pub struct BotConnection {
    pub username: String,
    pub server_host: String,
    pub server_port: u16,
    /// UUID assigned by the server (for offline mode, this is usually
    /// derived from the username via OfflinePlayer:username).
    pub profile_id: Uuid,
}

/// Try to log into a Minecraft server.
///
/// Automatically uses offline mode or online mode based on the `account`.
/// Online mode is **not yet implemented** and will return an error.
pub async fn login(account: &Account, host: &str, port: u16, protocol_version: i32) -> Result<BotConnection> {
    match account {
        Account::Offline { username } => {
            login_offline(host, port, username, protocol_version).await
        }
        Account::Online { .. } => {
            anyhow::bail!(
                "Online mode login is not yet implemented. \
                 See Microsoft OAuth / Minecraft Services integration (TODO)"
            );
        }
    }
}

/// Try to log into a Minecraft server in offline mode.
///
/// This sends the handshake + login start, then waits for a response.
/// If the server accepts the login, it returns `BotConnection`.
/// If rejected, it returns an error with the disconnect reason.
pub async fn login_offline(
    host: &str,
    port: u16,
    username: &str,
    protocol_version: i32,
) -> Result<BotConnection> {
    let addr = format!("{}:{}", host, port);
    let mut stream =
        TcpStream::connect(&addr)
            .await
            .map_err(|e| anyhow!("TCP connect to {addr}: {e}"))?;

    // --- Handshake (packet ID 0x00) ---
    let mut payload = Vec::new();
    write_varint(&mut payload, 0x00); // packet ID
    write_varint(&mut payload, protocol_version);
    write_string(&mut payload, host);
    write_u16(&mut payload, port);
    write_varint(&mut payload, 2); // next_state = login

    let handshake = packet_frame(&payload);
    stream
        .write_all(&handshake)
        .await
        .map_err(|e| anyhow!("write handshake: {e}"))?;

    // --- Login Start (packet ID 0x00 in login state) ---
    let mut login_payload = Vec::new();
    write_varint(&mut login_payload, 0x00); // packet ID
    write_string(&mut login_payload, &username[..username.len().min(16)]);
    write_uuid(&mut login_payload, &Uuid::nil()); // profile_id for offline

    let login_frame = packet_frame(&login_payload);
    stream
        .write_all(&login_frame)
        .await
        .map_err(|e| anyhow!("write login start: {e}"))?;

    // --- Read response ---
    let response = read_packet(&mut stream)
        .await
        .context("read login response")?;

    // The response is: VarInt packet_id + payload
    let mut cursor = Cursor::new(&response[..]);
    let packet_id = read_varint(&mut cursor).context("read packet id")?;

    match packet_id {
        0x00 => {
            // Login Disconnect — read the reason string
            let reason =
                read_varint_string(&mut cursor).context("read disconnect reason")?;
            anyhow::bail!("Server rejected login: {reason}");
        }
        0x02 => {
            // Login Finished — read GameProfile (uuid + name) + session_id
            let profile_uuid = read_uuid_bytes(&mut cursor)?;
            let profile_name = read_varint_string(&mut cursor)?;
            let _session_id = read_uuid_bytes(&mut cursor)?;

            log::info!(
                "Bot '{}' logged in as {} ({})",
                username,
                profile_name,
                profile_uuid
            );

            // We're in!  Clean shutdown
            let _ = stream.shutdown().await;

            Ok(BotConnection {
                username: profile_name,
                server_host: host.to_owned(),
                server_port: port,
                profile_id: profile_uuid,
            })
        }
        other => {
            anyhow::bail!(
                "Unexpected packet ID 0x{other:02x} during login (expected 0x00 or 0x02)"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Sync varint / string / uuid helpers for reading from Cursor
// ---------------------------------------------------------------------------

pub fn read_varint_string(buf: &mut Cursor<&[u8]>) -> Result<String> {
    let len = read_varint(buf)?;
    let mut bytes = vec![0u8; len as usize];
    Read::read_exact(buf, &mut bytes).context("read string")?;
    String::from_utf8(bytes).context("invalid UTF-8 in string")
}

pub fn read_uuid_bytes(buf: &mut Cursor<&[u8]>) -> Result<Uuid> {
    let mut bytes = [0u8; 16];
    Read::read_exact(buf, &mut bytes).context("read uuid")?;
    Ok(Uuid::from_bytes(bytes))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        for val in [0, 1, 127, 128, 255, 65535, 775, -1, i32::MAX, i32::MIN] {
            let mut buf = Vec::new();
            write_varint(&mut buf, val);
            let mut cursor = Cursor::new(&buf[..]);
            let read = read_varint(&mut cursor).unwrap();
            assert_eq!(val, read, "VarInt roundtrip failed for {val}");
        }
    }

    #[tokio::test]
    async fn test_login_offline_localhost_refused() {
        let result = login_offline("127.0.0.1", 25566, "TestBot", 775).await;
        assert!(result.is_err(), "Expected failure for offline server");
    }

    /// Try to actually log into a known-working server.
    /// This is an integration test — mark with `--ignored` so it only runs
    /// when explicitly requested.
    #[tokio::test]
    #[ignore]
    async fn connect_real_server() {
        // Replace with your own target for testing
        let r = login_offline("127.0.0.1", 25565, "TestBot", 775).await;
        eprintln!("connect_real_server: {r:#?}");
        match &r {
            Ok(conn) => {
                println!("✅ LOGGED IN! UUID={} Name={}", conn.profile_id, conn.username);
            }
            Err(e) => {
                println!("❌ FAILED: {e:#}");
            }
        }
        assert!(r.is_ok(), "Bot should be able to log in");
    }
}
