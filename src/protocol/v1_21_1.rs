use std::sync::Arc;
use crate::protocol::{BotSession, GameProfile, GameVersion, ProxyInfo, SessionListener, UniversalProtocol};
use crate::real_session::RealSession;

/// 1.21+ protocol wrapper (GeyserMC MCProtocolLib).
/// Reference: reference/mcprotocollib/ (tag 1.21-1 for MinecraftCodec with protocol 767)
pub struct ProtocolWrapper { profile: GameProfile }

impl ProtocolWrapper {
    pub fn new(username: &str) -> Self { Self { profile: GameProfile::new(username) } }
}

impl UniversalProtocol for ProtocolWrapper {
    fn profile(&self) -> &GameProfile { &self.profile }
    fn game_version(&self) -> GameVersion { GameVersion::V1_21_1 }
    fn connect(&self, host: &str, port: u16, proxy: Option<&ProxyInfo>, listener: Arc<dyn SessionListener>) -> Box<dyn BotSession> {
        let username = self.profile.name.clone();
        let host_owned = host.to_owned();
        let proxy_owned = proxy.cloned();

        // Use block_on because BotSession::connect is sync but we need async I/O.
        // We're inside a tokio runtime, so this is safe.
        match tokio::runtime::Handle::current().block_on(RealSession::connect(
            &host_owned, port, &username, GameVersion::V1_21_1,
            proxy_owned.as_ref(), listener,
        )) {
            Some(session) => Box::new(session),
            None => {
                log::error!("[{}] Failed to connect to {}:{}", username, host, port);
                Box::new(StubSession { username: username.clone(), connected: false })
            }
        }
    }
}

/// Fallback stub when real connection fails (also used by unsupported versions).
#[allow(dead_code)]
struct StubSession { username: String, connected: bool }

impl BotSession for StubSession {
    fn is_connected(&self) -> bool { self.connected }
    fn send_chat(&mut self, msg: &str) { log::info!("[{}] Chat (stub): {}", self.username, msg); }
    fn disconnect(&mut self, _reason: &str) { self.connected = false; }
}