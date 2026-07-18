//! Real MC protocol session — connects via TCP, handles login, keepalive, and chat.
//!
//! Bridge between `bot_connect.rs` (low-level TCP login) and the `BotSession` trait.
//! Spawns a background reader task for keepalive + incoming packets.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::Cursor;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::bot_connect;
use crate::protocol::{BotSession, GameVersion, ProxyInfo, SessionListener};

// ---------------------------------------------------------------------------
// Packet ID tables (PLAY state, per version)
// Keepalive handling is required for the connection to stay alive.
// Chat message is the primary bot operation.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct VersionPackets {
    /// Serverbound: chat message
    chat_message: i32,
    /// Serverbound: keepalive response
    keepalive_response: i32,
    /// Clientbound: keepalive request
    keepalive_request: i32,
    /// Clientbound: system chat message
    system_chat: i32,
    /// Serverbound: disconnect
    disconnect: i32,
}

/// Look up packet IDs for a given version. Returns None for unsupported versions.
fn packet_ids(version: GameVersion) -> Option<VersionPackets> {
    Some(match version {
        // 1.21 – 1.21.1 (protocol 767)
        GameVersion::V1_21_0 | GameVersion::V1_21_1 => VersionPackets {
            chat_message: 0x04,
            keepalive_response: 0x18,
            keepalive_request: 0x26,
            system_chat: 0x36,
            disconnect: 0x1A,
        },
        // 1.21.3 (protocol 768)
        GameVersion::V1_21_3 => VersionPackets {
            chat_message: 0x04,
            keepalive_response: 0x18,
            keepalive_request: 0x26,
            system_chat: 0x36,
            disconnect: 0x1A,
        },
        // 1.21.4 (protocol 769)
        GameVersion::V1_21_4 => VersionPackets {
            chat_message: 0x04,
            keepalive_response: 0x18,
            keepalive_request: 0x26,
            system_chat: 0x36,
            disconnect: 0x1A,
        },
        // 1.20.x (protocol 763–766) — roughly same layout
        GameVersion::V1_20 | GameVersion::V1_20_2 | GameVersion::V1_20_6 => VersionPackets {
            chat_message: 0x04,
            keepalive_response: 0x15,
            keepalive_request: 0x23,
            system_chat: 0x36,
            disconnect: 0x1A,
        },
        // 1.19.x (protocol 759–762)
        GameVersion::V1_19 | GameVersion::V1_19_2 | GameVersion::V1_19_4 => VersionPackets {
            chat_message: 0x04,
            keepalive_response: 0x15,
            keepalive_request: 0x23,
            system_chat: 0x36,
            disconnect: 0x1A,
        },
        // 1.18.x (protocol 757–758)
        GameVersion::V1_18 | GameVersion::V1_18_2 => VersionPackets {
            chat_message: 0x04,
            keepalive_response: 0x15,
            keepalive_request: 0x23,
            system_chat: 0x36,
            disconnect: 0x1A,
        },
        // 1.17.1 (protocol 756)
        GameVersion::V1_17_1 => VersionPackets {
            chat_message: 0x04,
            keepalive_response: 0x15,
            keepalive_request: 0x23,
            system_chat: 0x36,
            disconnect: 0x1A,
        },
        // Older versions (1.11–1.16.5) — different packet layout, not yet supported
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// RealSession
// ---------------------------------------------------------------------------

/// A real Minecraft bot session that connects to a server via TCP,
/// performs the login handshake, and keeps the connection alive.
pub struct RealSession {
    _username: String,
    writer: Option<Arc<Mutex<OwnedWriteHalf>>>,
    connected: Arc<AtomicBool>,
    listener: Option<Arc<dyn SessionListener>>,
    packets: Option<VersionPackets>,
}

impl RealSession {
    /// Connect to a server and perform the login handshake.
    /// Returns `None` if the version is not supported or login fails.
    pub async fn connect(
        host: &str,
        port: u16,
        username: &str,
        version: GameVersion,
        _proxy: Option<&ProxyInfo>,
        listener: Arc<dyn SessionListener>,
    ) -> Option<Self> {
        let packets = packet_ids(version)?;
        let proto_ver = version.protocol_version().unwrap_or(-1);

        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr).await.ok()?;
        log::info!("[{}] TCP connected to {}:{}", username, host, port);

        // --- Handshake (packet ID 0x00) ---
        let mut payload = Vec::new();
        bot_connect::write_varint(&mut payload, 0x00);
        bot_connect::write_varint(&mut payload, proto_ver);
        bot_connect::write_string(&mut payload, host);
        bot_connect::write_u16(&mut payload, port);
        bot_connect::write_varint(&mut payload, 2); // next_state = login

        let mut stream_ref = stream;
        let frame = bot_connect::packet_frame(&payload);
        if stream_ref.write_all(&frame).await.is_err() {
            log::error!("[{}] Failed to send handshake", username);
            return None;
        }

        // --- Login Start (packet ID 0x00 in login state) ---
        let mut login_payload = Vec::new();
        bot_connect::write_varint(&mut login_payload, 0x00);
        bot_connect::write_string(&mut login_payload, &username[..username.len().min(16)]);
        bot_connect::write_uuid(&mut login_payload, &Uuid::nil());

        let login_frame = bot_connect::packet_frame(&login_payload);
        if stream_ref.write_all(&login_frame).await.is_err() {
            log::error!("[{}] Failed to send login start", username);
            return None;
        }

        // --- Read login response ---
        let response = match bot_connect::read_packet(&mut stream_ref).await {
            Ok(r) => r,
            Err(e) => {
                log::error!("[{}] Login response error: {}", username, e);
                return None;
            }
        };

        let mut cursor = Cursor::new(&response[..]);
        let packet_id = bot_connect::read_varint(&mut cursor).ok()?;

        match packet_id {
            0x00 => {
                // Login Disconnect
                let reason = bot_connect::read_varint_string(&mut cursor).ok()
                    .unwrap_or_else(|| "unknown".into());
                log::warn!("[{}] Server rejected login: {}", username, reason);
                listener.on_disconnected(&reason);
                return None;
            }
            0x02 => {
                // Login Success
                let _profile_uuid = bot_connect::read_uuid_bytes(&mut cursor).ok()?;
                let _profile_name = bot_connect::read_varint_string(&mut cursor).ok()?;
                log::info!("[{}] Login successful", username);
            }
            0x03 => {
                // Login Plugin Request — skip for now
                log::warn!("[{}] Login plugin request received (not handled)", username);
            }
            other => {
                log::warn!("[{}] Unexpected login packet 0x{other:02x}", username);
            }
        }

        // Split stream: reader task handles keepalive, writer shared via Mutex
        let (reader, writer) = stream_ref.into_split();
        let writer = Arc::new(Mutex::new(writer));
        let connected = Arc::new(AtomicBool::new(true));
        let reader_writer = writer.clone();
        let reader_connected = connected.clone();
        let reader_listener = listener.clone();
        let reader_username = username.to_string();
        let reader_packets = packets.clone();

        // Spawn background reader task for keepalive + incoming packets
        tokio::spawn(async move {
            Self::reader_loop(
                reader,
                reader_writer,
                reader_username,
                reader_packets,
                reader_connected,
                reader_listener,
            ).await;
        });

        listener.on_join();

        Some(Self {
            _username: username.to_string(),
            writer: Some(writer),
            connected,
            listener: Some(listener),
            packets: Some(packets),
        })
    }

    /// Background reader: handles keepalive, chat, and disconnect packets.
    async fn reader_loop(
        mut reader: OwnedReadHalf,
        writer: Arc<Mutex<OwnedWriteHalf>>,
        _username: String,
        packets: VersionPackets,
        connected: Arc<AtomicBool>,
        listener: Arc<dyn SessionListener>,
    ) {
        loop {
            let mut raw = match Self::read_one_packet(&mut reader).await {
                Ok(p) => p,
                Err(_) => {
                    connected.store(false, Ordering::SeqCst);
                    listener.on_disconnected("connection lost");
                    return;
                }
            };

            // Parse packet ID via VarInt from a copy of the raw data
            let packet_id = {
                let mut tmp_cursor = Cursor::new(&raw[..]);
                match bot_connect::read_varint(&mut tmp_cursor) {
                    Ok(id) => {
                        // Calculate how many bytes the VarInt took
                        let id_len = raw.len() - tmp_cursor.get_ref().len() + tmp_cursor.position() as usize;
                        raw.drain(..id_len);
                        id
                    }
                    Err(_) => continue,
                }
            };

            match packet_id {
                id if id == packets.keepalive_request => {
                    // Keepalive request: send back the same payload
                    let mut resp = Vec::new();
                    bot_connect::write_varint(&mut resp, packets.keepalive_response);
                    resp.extend_from_slice(&raw);
                    let frame = bot_connect::packet_frame(&resp);
                    let mut w = writer.lock().await;
                    let _ = w.write_all(&frame).await;
                }
                id if id == packets.system_chat => {
                    // System chat message: try to parse the JSON message
                    let mut data_cursor = Cursor::new(&raw[..]);
                    let msg = match bot_connect::read_varint_string(&mut data_cursor) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    listener.on_chat_message(&msg);
                }
                id if id == 0x1E || id == 0x1A => {
                    // Disconnect (PLAY) or Login Disconnect
                    let mut data_cursor = Cursor::new(&raw[..]);
                    let reason = bot_connect::read_varint_string(&mut data_cursor)
                        .unwrap_or_else(|_| "unknown".into());
                    connected.store(false, Ordering::SeqCst);
                    listener.on_disconnected(&reason);
                    return;
                }
                _ => {
                    // Unknown packet — ignore for now
                }
            }
        }
    }

    async fn read_one_packet(reader: &mut OwnedReadHalf) -> Result<Vec<u8>> {
        // Read VarInt length directly from OwnedReadHalf
        let mut value: u32 = 0;
        let mut position = 0;
        loop {
            let mut byte = [0u8; 1];
            reader.read_exact(&mut byte).await?;
            value |= ((byte[0] & 0x7Fu8) as u32) << position;
            if (byte[0] & 0x80u8) == 0 {
                break;
            }
            position += 7;
            if position >= 32 {
                anyhow::bail!("VarInt too big");
            }
        }
        let length = value as i32;
        let mut payload = vec![0u8; length as usize];
        reader.read_exact(&mut payload).await?;
        Ok(payload)
    }
}

impl BotSession for RealSession {
    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn send_chat(&mut self, msg: &str) {
        let packets = match self.packets {
            Some(ref p) => p,
            None => return,
        };
        if let Some(ref writer) = self.writer {
            let mut payload = Vec::new();
            bot_connect::write_varint(&mut payload, packets.chat_message);
            // Chat message body: a JSON text component string
            let json = format!("{{\"text\":\"{}\"}}", msg.replace('\\', "\\\\").replace('"', "\\\""));
            bot_connect::write_string(&mut payload, &json);
            let frame = bot_connect::packet_frame(&payload);
            let mut w = tokio::runtime::Handle::current()
                .block_on(writer.lock());
            let _ = tokio::runtime::Handle::current()
                .block_on(w.write_all(&frame));
        }
    }

    fn disconnect(&mut self, reason: &str) {
        let packets = match self.packets {
            Some(ref p) => p,
            None => return,
        };
        if let Some(ref writer) = self.writer {
            let mut payload = Vec::new();
            bot_connect::write_varint(&mut payload, packets.disconnect);
            let json = format!("{{\"text\":\"{}\"}}", reason);
            bot_connect::write_string(&mut payload, &json);
            let frame = bot_connect::packet_frame(&payload);
            let mut w = tokio::runtime::Handle::current()
                .block_on(writer.lock());
            let _ = tokio::runtime::Handle::current()
                .block_on(w.write_all(&frame));
        }
        self.connected.store(false, Ordering::SeqCst);
        if let Some(ref listener) = self.listener {
            listener.on_disconnected(reason);
        }
    }
}

impl Drop for RealSession {
    fn drop(&mut self) {
        if self.connected.load(Ordering::SeqCst) {
            self.disconnect("session dropped");
        }
    }
}