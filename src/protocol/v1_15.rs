use std::sync::Arc;
use crate::protocol::{BotSession, GameProfile, GameVersion, ProxyInfo, SessionListener, UniversalProtocol};
use crate::real_session::RealSession;

/// 1.15.2 protocol wrapper (old Steveice10 MCProtocolLib).
pub struct ProtocolWrapper { profile: GameProfile }

impl ProtocolWrapper {
    pub fn new(username: &str) -> Self { Self { profile: GameProfile::new(username) } }
}

impl UniversalProtocol for ProtocolWrapper {
    fn profile(&self) -> &GameProfile { &self.profile }
    fn game_version(&self) -> GameVersion { GameVersion::V1_15_2 }
    fn connect(&self, host: &str, port: u16, proxy: Option<&ProxyInfo>, listener: Arc<dyn SessionListener>) -> Box<dyn BotSession> {
        let username = self.profile.name.clone();
        log::info!("[{}] Connecting {}:{} (1.15.2)", username, host, port);
        match tokio::runtime::Handle::current().block_on(RealSession::connect(
            host, port, &username, GameVersion::V1_15_2,
            proxy, listener,
        )) {
            Some(session) => Box::new(session),
            None => {
                log::warn!("[{}] 1.15.2 real session failed, using stub", username);
                Box::new(StubSession { username, connected: false })
            }
        }
    }
}

#[allow(dead_code)]
struct StubSession { username: String, connected: bool }

impl BotSession for StubSession {
    fn is_connected(&self) -> bool { self.connected }
    fn send_chat(&mut self, msg: &str) { log::info!("[{}] Chat (stub): {}", self.username, msg); }
    fn disconnect(&mut self, reason: &str) { self.connected = false; }
}