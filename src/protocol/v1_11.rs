use std::sync::Arc;
use crate::protocol::{BotSession, GameProfile, GameVersion, ProxyInfo, SessionListener, UniversalProtocol};

/// 1.11 protocol wrapper (old Steveice10 MCProtocolLib).
/// Reference: reference/mc-bots-ref/
pub struct ProtocolWrapper { profile: GameProfile }

impl ProtocolWrapper {
    pub fn new(username: &str) -> Self { Self { profile: GameProfile::new(username) } }
}

impl UniversalProtocol for ProtocolWrapper {
    fn profile(&self) -> &GameProfile { &self.profile }
    fn game_version(&self) -> GameVersion { GameVersion::V1_11 }
    fn connect(&self, host: &str, port: u16, _proxy: Option<&ProxyInfo>, listener: Arc<dyn SessionListener>) -> Box<dyn BotSession> {
        log::info!("[{}] Connecting {}:{} (1.11)", self.profile.name, host, port);
        Box::new(V1_11Session { username: self.profile.name.clone(), connected: true, listener })
    }
}

#[allow(dead_code)]
struct V1_11Session { username: String, connected: bool, listener: Arc<dyn SessionListener> }

impl BotSession for V1_11Session {
    fn is_connected(&self) -> bool { self.connected }
    fn send_chat(&mut self, msg: &str) { log::info!("[{}] Chat 1.11: {}", self.username, msg); }
    fn disconnect(&mut self, reason: &str) { self.connected = false; self.listener.on_disconnected(reason); }
}

impl Drop for V1_11Session { fn drop(&mut self) { if self.connected { self.disconnect("drop"); } } }
