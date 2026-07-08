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
    /// Hub (central node) settings — used by `status`, `hub`, `journal`, `tui`, `monitor` commands.
    #[serde(default)]
    pub hub: HubConfig,
}

/// Creeper Hub (central sync node) configuration.
/// Stored in config so commands like `creeper status` work without re-typing the URL each time.
///
/// 支持多个 hub 节点，每个节点可有多个连接方式（IPv4/IPv6/内网穿透/正向反向 HTTP/WS 等），
/// 按优先级自动选可用 URL。旧的单 `url` 字段仍可反序列化（向后兼容），自动迁移为
/// 一个含单连接的默认节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConfig {
    /// 默认 hub 节点的 id 或 label。CLI `--hub` 不指定时用此节点。
    /// 空 = 取 `nodes` 第一个。
    #[serde(default)]
    pub default_node: String,
    /// 多个 hub 节点。每个有唯一 id + 多个连接方式。
    #[serde(default)]
    pub nodes: Vec<HubNode>,
    /// **Deprecated** 旧单 URL 字段（向后兼容反序列化用）。不直接读取。
    /// 反序列化时若 `nodes` 为空而此字段非空，自动迁移成一个节点。
    #[serde(default, skip_serializing)]
    pub url: String,
}

/// 单个 hub 节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubNode {
    /// 唯一标识符。建议用 sha256（hostname:port 的 sha256 前 16 hex）或 uuid。
    /// 也可是人类可读 label（如 `scanner-node` / `probe-node`），CLI `--hub` 可用此匹配。
    pub id: String,
    /// 人类可读别名（可选）。CLI `--hub` 也会匹配此字段。
    #[serde(default)]
    pub label: String,
    /// 节点角色：`probe`（探针，仅监控）/ `scanner`（扫描）/ `hub`（中枢聚合）。
    /// Agent 据此判断可派什么任务。
    #[serde(default)]
    pub role: String,
    /// 多个连接方式，按 `priority` 升序选可用 URL。
    #[serde(default)]
    pub connections: Vec<HubConnection>,
}

/// 单个连接方式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConnection {
    /// 连接类型：
    /// - `ipv4`        IPv4 直连
    /// - `ipv6`        IPv6 直连
    /// - `nat`         内网穿透 IPv4 �口
    /// - `forward-http` 正向 HTTP（本机直连远端 HTTP）
    /// - `reverse-http` 反向 HTTP（远端反连本机暴露的 HTTP）
    /// - `forward-ws`   正向 WebSocket
    /// - `reverse-ws`   反向 WebSocket
    /// - `forward-tcp`  正向裸 TCP
    /// - `reverse-tcp`  反向裸 TCP
    /// - `tor`          Tor onion
    /// - `i2p`          I2P eepsite
    /// - `other`        其他自定义
    pub kind: String,
    /// 实际 URL（含 scheme 与端口）。如 `http://1.2.3.4:9090`、`http://[2001:db8::1]:9090`、
    /// `http://<your-nat-host>:<nat-port>`。CLI 自动用此值。
    pub url: String,
    /// 优先级（小者优先）。同节点多连接方式按此升序选第一个可用。默认 0。
    #[serde(default)]
    pub priority: i32,
    /// 是否启用。false 则跳过不选。默认 true。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 备注（可选）。如 "家里宽带直连" / "vultr 反代"。
    #[serde(default)]
    pub note: String,
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
impl Default for HubConfig { fn default() -> Self { Self { default_node: String::new(), nodes: vec![], url: String::new() } } }
impl Default for Config {
    fn default() -> Self {
        Self { target: Default::default(), bot: Default::default(), proxy: Default::default(),
               nickname: Default::default(), server: Default::default(), api: Default::default(),
               hub: Default::default() }
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

    /// Resolve the hub URL with precedence:
    ///   1. CLI override (`--hub <raw>`) — 若是节点 id/label 匹配则走该节点选可用连接，
    ///      否则当裸 URL 用（auto-prepend `http://`）。
    ///   2. config `hub.default_node` 指定节点 → 选该节点可用连接。
    ///   3. config `hub.nodes[0]` → 选该节点可用连接。
    ///   4. **Legacy** config `hub.url` 非空 → 直接用（向后兼容）。
    ///   5. built-in default `http://127.0.0.1:9090`。
    ///
    /// `cli_hub` 是 CLI `--hub` 值（可为节点 id/label/裸 URL）。`cli_url` 是老的位置参数 URL。
    /// Auto-prepends `http://` if no scheme is present, and strips trailing slashes.
    pub fn resolve_hub_url(&self, cli_hub: Option<&str>, cli_url: Option<&str>) -> String {
        const DEFAULT_HUB: &str = "http://127.0.0.1:9090";

        // 1. CLI --hub <id|label|url>
        if let Some(h) = cli_hub.filter(|s| !s.trim().is_empty()) {
            // 先按 id/label 匹配节点
            if let Some(node) = self.find_node(h) {
                if let Some(url) = self.pick_connection(node) {
                    return url;
                }
            }
            // 否则当裸 URL 用
            return normalize_url(h);
        }
        // 1b. CLI 老 positional URL
        if let Some(u) = cli_url.filter(|s| !s.trim().is_empty()) {
            return normalize_url(u);
        }
        // 2. config default_node
        if !self.hub.default_node.trim().is_empty() {
            if let Some(node) = self.find_node(&self.hub.default_node) {
                if let Some(url) = self.pick_connection(node) {
                    return url;
                }
            }
        }
        // 3. config nodes[0]
        if let Some(node) = self.hub.nodes.first() {
            if let Some(url) = self.pick_connection(node) {
                return url;
            }
        }
        // 4. legacy url
        let legacy = self.hub.url.trim();
        if !legacy.is_empty() {
            return normalize_url(legacy);
        }
        // 5. built-in default
        DEFAULT_HUB.to_string()
    }

    /// 按节点查询：匹配 id 或 label（精确匹配优先，其次 contains）。
    fn find_node(&self, key: &str) -> Option<&HubNode> {
        let k = key.trim();
        self.hub.nodes.iter().find(|n| n.id == k || n.label == k)
            .or_else(|| self.hub.nodes.iter().find(|n| n.id.contains(k) || n.label.contains(k)))
    }

    /// 按优先级选节点第一个 enabled 的连接方式 URL。无则 None。
    fn pick_connection(&self, node: &HubNode) -> Option<String> {
        let mut conns: Vec<&HubConnection> = node.connections.iter()
            .filter(|c| c.enabled)
            .collect();
        conns.sort_by_key(|c| c.priority);
        conns.first().map(|c| normalize_url(&c.url))
    }
}

fn normalize_url(raw: &str) -> String {
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{}", raw)
    };
    with_scheme.trim_end_matches('/').to_string()
}

pub fn print_default() {
    println!("{}", serde_json::to_string_pretty(&Config::default()).unwrap());
}
