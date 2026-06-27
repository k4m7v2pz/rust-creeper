//! Protocol abstraction layer — GameVersion enum, UniversalProtocol trait,
//! BotSession/SessionListener traits, and version-specific implementations.

pub mod v1_11;
pub mod v1_12;
pub mod v1_14;
pub mod v1_15;
pub mod v1_16;
pub mod v1_21_1;

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

/// Supported Minecraft versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameVersion {
    V1_11, V1_12, V1_12_1, V1_12_2,
    V1_14_4,
    V1_15, V1_15_2,
    V1_16_3, V1_16_4, V1_16_5,
    V1_17_1,
    V1_18, V1_18_2,
    V1_19, V1_19_2, V1_19_4,
    V1_20, V1_20_2, V1_20_6,
    V1_21_0, V1_21_1, V1_21_3, V1_21_4, V1_21_5, V1_21_6, V1_21_7,
}

impl GameVersion {
    pub fn find_by_name(name: &str) -> Option<Self> {
        match name {
            "1.11" => Some(Self::V1_11), "1.12" => Some(Self::V1_12),
            "1.12.1" => Some(Self::V1_12_1), "1.12.2" => Some(Self::V1_12_2),
            "1.14.4" => Some(Self::V1_14_4),
            "1.15" => Some(Self::V1_15), "1.15.2" => Some(Self::V1_15_2),
            "1.16.3" => Some(Self::V1_16_3), "1.16.4" => Some(Self::V1_16_4), "1.16.5" => Some(Self::V1_16_5),
            "1.17.1" => Some(Self::V1_17_1),
            "1.18" => Some(Self::V1_18), "1.18.2" => Some(Self::V1_18_2),
            "1.19" => Some(Self::V1_19), "1.19.2" => Some(Self::V1_19_2), "1.19.4" => Some(Self::V1_19_4),
            "1.20" => Some(Self::V1_20), "1.20.2" => Some(Self::V1_20_2), "1.20.6" => Some(Self::V1_20_6),
            "1.21" => Some(Self::V1_21_0), "1.21.1" => Some(Self::V1_21_1),
            "1.21.3" => Some(Self::V1_21_3), "1.21.4" => Some(Self::V1_21_4),
            "1.21.5" => Some(Self::V1_21_5), "1.21.6" => Some(Self::V1_21_6), "1.21.7" => Some(Self::V1_21_7),
            _ => None,
        }
    }

    pub fn version_str(&self) -> &'static str {
        match self {
            Self::V1_11 => "1.11", Self::V1_12 => "1.12", Self::V1_12_1 => "1.12.1", Self::V1_12_2 => "1.12.2",
            Self::V1_14_4 => "1.14.4",
            Self::V1_15 => "1.15", Self::V1_15_2 => "1.15.2",
            Self::V1_16_3 => "1.16.3", Self::V1_16_4 => "1.16.4", Self::V1_16_5 => "1.16.5",
            Self::V1_17_1 => "1.17.1",
            Self::V1_18 => "1.18", Self::V1_18_2 => "1.18.2",
            Self::V1_19 => "1.19", Self::V1_19_2 => "1.19.2", Self::V1_19_4 => "1.19.4",
            Self::V1_20 => "1.20", Self::V1_20_2 => "1.20.2", Self::V1_20_6 => "1.20.6",
            Self::V1_21_0 => "1.21", Self::V1_21_1 => "1.21.1",
            Self::V1_21_3 => "1.21.3", Self::V1_21_4 => "1.21.4",
            Self::V1_21_5 => "1.21.5", Self::V1_21_6 => "1.21.6", Self::V1_21_7 => "1.21.7",
        }
    }

    pub fn protocol_version(&self) -> Option<i32> {
        match self {
            Self::V1_18 => Some(757), Self::V1_18_2 => Some(758),
            Self::V1_19 => Some(759), Self::V1_19_2 => Some(760), Self::V1_19_4 => Some(762),
            Self::V1_20 => Some(763), Self::V1_20_2 => Some(764), Self::V1_20_6 => Some(766),
            Self::V1_21_0 => Some(767), Self::V1_21_1 => Some(767),
            Self::V1_21_3 => Some(768), Self::V1_21_4 => Some(769),
            Self::V1_21_5 => Some(770), Self::V1_21_6 => Some(771), Self::V1_21_7 => Some(772),
            _ => None,
        }
    }

    pub fn all() -> &'static [GameVersion] {
        &[Self::V1_11, Self::V1_12, Self::V1_12_1, Self::V1_12_2, Self::V1_14_4,
          Self::V1_15, Self::V1_15_2, Self::V1_16_3, Self::V1_16_4, Self::V1_16_5,
          Self::V1_17_1, Self::V1_18, Self::V1_18_2, Self::V1_19, Self::V1_19_2,
          Self::V1_19_4, Self::V1_20, Self::V1_20_2, Self::V1_20_6,
          Self::V1_21_0, Self::V1_21_1, Self::V1_21_3, Self::V1_21_4,
          Self::V1_21_5, Self::V1_21_6, Self::V1_21_7]
    }
}

impl fmt::Display for GameVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.version_str()) }
}

#[derive(Debug, Clone)]
pub struct GameProfile { pub uuid: Uuid, pub name: String }

impl GameProfile {
    pub fn new(name: &str) -> Self { Self { uuid: Uuid::new_v4(), name: name.to_owned() } }
}

/// Proxy information with protocol type.
/// Supports SOCKS4, SOCKS5, HTTP via `proxied` crate.
/// Format: `protocol://[user:pass@]host:port`
#[derive(Debug, Clone)]
pub struct ProxyInfo {
    pub url: String,
}

impl ProxyInfo {
    /// Parse a proxy URL string. Supported formats:
    ///   socks4://host:port
    ///   socks5://[user:pass@]host:port
    ///   http://[user:pass@]host:port
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if !input.contains("://") {
            return Err("Missing protocol:// prefix. Use socks4:// socks5:// or http://".into());
        }
        Ok(Self { url: input.to_owned() })
    }

    /// Create a proxied `Proxy` for actual connection.
    pub fn to_proxied(&self) -> Result<proxied::Proxy, String> {
        proxied::Proxy::from_str(&self.url)
            .map_err(|e| format!("Invalid proxy '{}': {}", self.url, e))
    }
}

impl std::str::FromStr for ProxyInfo {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> { Self::parse(s) }
}

/// Active bot connection.
pub trait BotSession: Send + Sync {
    fn is_connected(&self) -> bool;
    fn send_chat(&mut self, message: &str);
    fn disconnect(&mut self, reason: &str);
}

/// Packet event callbacks.
pub trait SessionListener: Send + Sync {
    fn on_disconnected(&self, reason: &str);
    fn on_join(&self);
    fn on_chat_message(&self, message: &str);
    fn on_position_update(&self, x: f64, y: f64, z: f64, pitch: f32, yaw: f32);
    fn on_health_update(&self, health: f32, food: f32);
}

/// Protocol abstraction — create a session for a specific version.
pub trait UniversalProtocol: Send + Sync {
    fn profile(&self) -> &GameProfile;
    fn game_version(&self) -> GameVersion;
    fn connect(&self, host: &str, port: u16, proxy: Option<&ProxyInfo>, listener: Arc<dyn SessionListener>) -> Box<dyn BotSession>;
}
