use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Main configuration — saved as JSON in `$XDG_CONFIG_HOME/creeper/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub target: TargetConfig,
    #[serde(default)]
    pub bot: BotConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub nickname: NicknameConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub api: ApiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default = "default_delay")]
    pub join_delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    #[serde(default = "default_name_format")]
    pub name_format: String,
    #[serde(default)]
    pub auto_register: bool,
    #[serde(default)]
    pub join_commands: Vec<String>,
    #[serde(default = "default_respawn")]
    pub auto_respawn_delay_ms: i64,
    /// Max connection attempts per bot (retry if kicked/disconnected).
    #[serde(default = "default_attempts")]
    pub max_attempts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy URLs (socks4:// socks5:// http:// ). One per entry or one per line in file.
    #[serde(default)]
    pub list: Vec<String>,
    /// Path to proxy file (one URL per line).
    #[serde(default)]
    pub file: Option<String>,
    /// URL to fetch proxy list from.
    #[serde(default)]
    pub url: Option<String>,
    /// Verify proxies by pinging target before use.
    #[serde(default = "default_true")]
    pub verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NicknameConfig {
    #[serde(default)]
    pub realistic: bool,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default = "default_nick_len")]
    pub length: usize,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_true")]
    pub ping_on_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_api_bind")]
    pub bind: String,
    #[serde(default = "default_api_port")]
    pub port: u16,
}

// —————— defaults ——————

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 25565 }
fn default_version() -> String { "1.21.1".into() }
fn default_count() -> usize { 20 }
fn default_delay() -> u64 { 1000 }
fn default_name_format() -> String { "Bot-%d".into() }
fn default_respawn() -> i64 { -1 }
fn default_attempts() -> usize { 1 }
fn default_nick_len() -> usize { 16 }
fn default_true() -> bool { true }
fn default_api_bind() -> String { "127.0.0.1".into() }
fn default_api_port() -> u16 { 9720 }

impl Default for TargetConfig {
    fn default() -> Self {
        Self { host: default_host(), port: default_port(), version: default_version(),
               count: default_count(), join_delay_ms: default_delay() }
    }
}
impl Default for BotConfig {
    fn default() -> Self {
        Self { name_format: default_name_format(), auto_register: false,
               join_commands: vec![], auto_respawn_delay_ms: default_respawn(), max_attempts: default_attempts() }
    }
}
impl Default for ProxyConfig { fn default() -> Self { Self { list: vec![], file: None, url: None, verify: true } } }
impl Default for NicknameConfig { fn default() -> Self { Self { realistic: false, file: None, length: default_nick_len(), prefix: String::new() } } }
impl Default for ServerConfig { fn default() -> Self { Self { ping_on_start: default_true() } } }
impl Default for ApiConfig { fn default() -> Self { Self { enabled: false, bind: default_api_bind(), port: default_api_port() } } }
impl Default for Config {
    fn default() -> Self {
        Self { target: Default::default(), bot: Default::default(), proxy: Default::default(),
               nickname: Default::default(), server: Default::default(), api: Default::default() }
    }
}

impl Config {
    pub fn path() -> anyhow::Result<PathBuf> {
        let dir = directories::ProjectDirs::from("", "", "creeper")
            .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory"))?;
        Ok(dir.config_dir().join("config.json"))
    }

    pub fn load() -> Self { Self::load_from(None).unwrap_or_default() }

    pub fn load_from(path: Option<&std::path::Path>) -> anyhow::Result<Self> {
        let p = match path { Some(p) => p.to_path_buf(), None => Self::path()? };
        let content = std::fs::read_to_string(&p)
            .map_err(|e| anyhow::anyhow!("Cannot read {}: {}", p.display(), e))?;
        Ok(serde_json::from_str(&content).map_err(|e| anyhow::anyhow!("Invalid JSON: {}", e))?)
    }

    pub fn save(&self) -> anyhow::Result<()> { self.save_to(&Self::path()?) }

    pub fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn apply_cli_overrides(&mut self, host: Option<String>, port: Option<u16>,
                                count: Option<usize>, delay: Option<u64>,
                                name_format: Option<String>, version: Option<String>,
                                auto_register: Option<bool>) {
        if let Some(v) = host { self.target.host = v; }
        if let Some(v) = port { self.target.port = v; }
        if let Some(v) = count { self.target.count = v; }
        if let Some(v) = delay { self.target.join_delay_ms = v; }
        if let Some(v) = name_format { self.bot.name_format = v; }
        if let Some(v) = version { self.target.version = v; }
        if let Some(v) = auto_register { self.bot.auto_register = v; }
    }
}

pub fn print_default() {
    println!("{}", serde_json::to_string_pretty(&Config::default()).unwrap());
}
