//! MCP (Model Context Protocol) 服务器。
//!
//! 将 creeper 的**只读数据面**（hub 查询）与**单服探针**暴露为 MCP tools，
//! 供 MCP 客户端（豆包连接器 / AtomCode / Claude / Cursor 等）接入调用。
//!
//! 运行：`creeper mcp`（默认 stdio）；`creeper mcp --http --addr 127.0.0.1:3001`
//!       起 HTTP（Streamable HTTP）监听。**HTTP 默认不开监听、不立服务**，
//!       只有显式 `--http` 才绑定端口。
//! 客户端配置：`{ "command": "<install>/bin/creeper", "args": ["mcp"] }`
//!
//! 实现参考：
//! - `rust-backup-tool/src/mcp.rs`（rmcp 3.1.2 + 手动 `ServerHandler`，stdio 验证可用）
//! - `ouchn-toolkit/src/mcp.rs`（同样 rmcp 3.1.2；3.4.0 宏模式在 stdio 下存在
//!   initialize 后提前退出问题，故锁定 3.1.2 并手动实现）
//!
//! 设计要点：
//! 1. **stdio stdout 纯净**：MCP 的 JSON-RPC 走 stdin/stdout，任何 `println!`
//!    都会污染协议流——工具返回结构化 JSON 文本，日志一律走 stderr。
//! 2. **只读边界**：只暴露 hub 查询（journal/servers/status/host-report/info）
//!    与单服探针（MOTD/版本/玩家数）。**不暴露**攻击类（stress/flood/attack
//!    链）、扫描发现类（discover/scan/crawl）、抹盘类（wipe）、任务下发类
//!    （hub submit/worker）——这些不适合 Agent 半自动调用。
//! 3. **与 CLI 共享执行路径**：probe 复用 `motd::ping`；hub 查询直接 HTTP GET
//!    hub 端点（与 AGENTS.md「实时数据源」一致），hub URL 缺省从 config 解析。
//! 4. **调用留痕**：每次工具调用追加到 `~/.cache/creeper/mcp-audit.jsonl`
//!    （who/what/ok/message/elapsed），供 Agent 排查自己的调用链。
//! 5. **transport**：默认 stdio（进程内直接会话）；HTTP 用 rmcp 的
//!    `StreamableHttpService` 挂到 axum（项目已有），路径 `/mcp`。

use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::stdio;
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value as JsonValue};
use std::sync::Arc;
use std::time::Instant;

/// MCP Server：持有暴露的工具清单。
pub struct CreeperMcp {
    tools: Vec<Tool>,
}

impl CreeperMcp {
    pub fn new() -> Self {
        let tools = vec![
            tool(
                "hub_info",
                "查询 hub 节点元信息与端点列表（GET /）。hub 缺省从本地 config 解析，\
                 解析不到时用 http://localhost:9090。",
                &[("hub", "string", "hub 根 URL（如 http://<host>:9090），可选", false)],
            ),
            tool(
                "journal",
                "拉取 hub 全部已发现服务器 + 玩家名册（GET /journal）。\
                 可选 limit 限制返回条目数。",
                &[
                    ("hub", "string", "hub 根 URL，可选", false),
                    ("limit", "integer", "限制返回的服务器/玩家条目数（默认 100）", false),
                ],
            ),
            tool(
                "servers",
                "拉取 hub 服务器元数据（MOTD/版本/在线人数等，GET /servers）。\
                 可选 host 按地址子串过滤。",
                &[
                    ("hub", "string", "hub 根 URL，可选", false),
                    ("host", "string", "按服务器地址（host:port）子串过滤，可选", false),
                ],
            ),
            tool(
                "hub_status",
                "hub 健康检查与统计（GET /status）。",
                &[("hub", "string", "hub 根 URL，可选", false)],
            ),
            tool(
                "host_report",
                "hub 上的本机资源探针记录（GET /host-report）。可选 limit 限制条数。",
                &[
                    ("hub", "string", "hub 根 URL，可选", false),
                    ("limit", "integer", "限制返回条数（默认 20）", false),
                ],
            ),
            tool(
                "probe_server",
                "单次探一个 Minecraft 服务器的 MOTD/版本/玩家数/玩家名册\
                 （Java 优先，回退 Bedrock）。只读查询，不发起任何攻击。",
                &[
                    ("host", "string", "服务器主机名或地址", true),
                    ("port", "integer", "端口（默认 25565）", false),
                ],
            ),
        ];
        Self { tools }
    }
}

impl Default for CreeperMcp {
    fn default() -> Self {
        Self::new()
    }
}

/// 从参数元组生成 MCP Tool（JSON Schema object）。
/// params: (key, type, description, required)
fn tool(name: &str, description: &str, params: &[(&str, &str, &str, bool)]) -> Tool {
    let mut props = Map::new();
    let mut required = Vec::new();
    for (key, ty, desc, req) in params {
        let mut prop = Map::new();
        prop.insert("type".into(), json!(ty));
        prop.insert("description".into(), json!(desc));
        props.insert(key.to_string(), JsonValue::Object(prop));
        if *req {
            required.push(json!(key));
        }
    }
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), JsonValue::Object(props));
    if !required.is_empty() {
        schema.insert("required".into(), JsonValue::Array(required));
    }
    Tool::new(name.to_string(), description.to_string(), Arc::new(schema))
}

/// 取必填字符串参数，缺失返回 invalid_params 错误。
fn required_str(args: &Map<String, JsonValue>, key: &str) -> Result<String, McpError> {
    args.get(key).and_then(json_to_string).ok_or_else(|| {
        McpError::invalid_params(format!("missing required parameter: {}", key), None)
    })
}

fn json_to_string(v: &JsonValue) -> Option<String> {
    match v {
        JsonValue::String(s) => Some(s.clone()),
        JsonValue::Number(n) => Some(n.to_string()),
        JsonValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn arg_str(args: &Map<String, JsonValue>, key: &str) -> Option<String> {
    args.get(key).and_then(json_to_string)
}

fn arg_int(args: &Map<String, JsonValue>, key: &str, default: usize) -> usize {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default)
}

/// 解析 hub URL：显式参数 → config 默认节点 → localhost 兜底。
fn resolve_hub(hub: Option<&str>) -> String {
    if let Some(raw) = hub {
        let t = raw.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let cfg = crate::config::Config::load();
    let url = cfg.resolve_hub_url(None, None);
    if url.trim().is_empty() {
        "http://localhost:9090".to_string()
    } else {
        url.trim().to_string()
    }
}

/// HTTP GET hub 端点，返回响应文本；非 2xx 视为错误。
async fn hub_get(hub: &str, path: &str) -> anyhow::Result<String> {
    let url = format!("{}{}", hub.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client.get(&url).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("hub {} -> HTTP {}", url, status);
    }
    Ok(text)
}

/// 审计：追加一行到 ~/.cache/creeper/mcp-audit.jsonl。
/// 失败静默（审计只是辅助，不阻塞工具调用）。
fn log_audit(
    tool_name: &str,
    args: &Map<String, JsonValue>,
    ok: bool,
    message: &str,
    elapsed_ms: u128,
) {
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let dir = home.join(".cache").join("creeper");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("mcp-audit.jsonl");
    let entry = json!({
        "when": chrono::Utc::now().to_rfc3339(),
        "tool": tool_name,
        "args": args,
        "ok": ok,
        "message": message,
        "elapsed_ms": elapsed_ms,
    });
    let line = serde_json::to_string(&entry).unwrap_or_default();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{}", line);
    }
}

impl ServerHandler for CreeperMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "creeper data plane via MCP (stdio/http). Tools: hub_info / journal / servers / \
                 hub_status / host_report / probe_server. 只读数据面 + 单服探针；\
                 不含任何攻击/抹盘/扫描发现类操作。hub 参数缺省从本地 config 解析。",
            )
            .with_server_info(Implementation::new("creeper-mcp", env!("CARGO_PKG_VERSION")))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tools.clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name.to_string();
        let arguments = request.arguments.unwrap_or_default();
        let start = Instant::now();
        let (ok, text): (bool, String) = match name.as_str() {
            "hub_info" => run_hub_info(&arguments).await,
            "journal" => run_journal(&arguments).await,
            "servers" => run_servers(&arguments).await,
            "hub_status" => run_hub_status(&arguments).await,
            "host_report" => run_host_report(&arguments).await,
            "probe_server" => run_probe_server(&arguments),
            other => {
                return Err(McpError::invalid_params(
                    format!("unknown tool: {}", other),
                    None,
                ))
            }
        };
        let elapsed_ms = start.elapsed().as_millis();
        log_audit(&name, &arguments, ok, &text, elapsed_ms);
        if ok {
            Ok(CallToolResponse::Complete(CallToolResult::success(vec![
                ContentBlock::text(text),
            ])))
        } else {
            Ok(CallToolResponse::Complete(CallToolResult::error(vec![
                ContentBlock::text(text),
            ])))
        }
    }
}

/// hub_info：GET / → 节点元信息 + 端点列表。
async fn run_hub_info(arguments: &Map<String, JsonValue>) -> (bool, String) {
    let hub = resolve_hub(arg_str(arguments, "hub").as_deref());
    match hub_get(&hub, "/").await {
        Ok(t) => (true, pretty(&t)),
        Err(e) => (false, format!("hub_info 失败: {}", e)),
    }
}

/// journal：GET /journal → 已发现服务器 + 玩家名册，可选 limit 截断。
async fn run_journal(arguments: &Map<String, JsonValue>) -> (bool, String) {
    let hub = resolve_hub(arg_str(arguments, "hub").as_deref());
    let limit = arg_int(arguments, "limit", 100);
    match hub_get(&hub, "/journal").await {
        Ok(t) => match serde_json::from_str::<JsonValue>(&t) {
            Ok(mut v) => {
                if let Some(players) = v.get_mut("players").and_then(|p| p.as_object_mut()) {
                    for list in players.values_mut() {
                        if let Some(arr) = list.as_array_mut() {
                            arr.truncate(limit);
                        }
                    }
                    // 服务器数量也截断
                    if players.len() > limit {
                        let keys: Vec<String> = players.keys().take(limit).cloned().collect();
                        players.retain(|k, _| keys.contains(k));
                    }
                }
                if let Some(info) = v.get_mut("server_info").and_then(|s| s.as_object_mut()) {
                    if info.len() > limit {
                        let keys: Vec<String> = info.keys().take(limit).cloned().collect();
                        info.retain(|k, _| keys.contains(k));
                    }
                }
                (true, pretty(&serde_json::to_string(&v).unwrap_or_default()))
            }
            Err(e) => (false, format!("journal 响应不是合法 JSON: {}", e)),
        },
        Err(e) => (false, format!("journal 失败: {}", e)),
    }
}

/// servers：GET /servers → 服务器元数据，可选 host 子串过滤。
async fn run_servers(arguments: &Map<String, JsonValue>) -> (bool, String) {
    let hub = resolve_hub(arg_str(arguments, "hub").as_deref());
    let filter = arg_str(arguments, "host").unwrap_or_default().to_lowercase();
    match hub_get(&hub, "/servers").await {
        Ok(t) => match serde_json::from_str::<JsonValue>(&t) {
            Ok(mut v) => {
                if !filter.is_empty() {
                    if let Some(map) = v.as_object_mut() {
                        map.retain(|k, _| k.to_lowercase().contains(&filter));
                    }
                }
                (true, pretty(&serde_json::to_string(&v).unwrap_or_default()))
            }
            Err(e) => (false, format!("servers 响应不是合法 JSON: {}", e)),
        },
        Err(e) => (false, format!("servers 失败: {}", e)),
    }
}

/// hub_status：GET /status → 健康检查 + 统计。
async fn run_hub_status(arguments: &Map<String, JsonValue>) -> (bool, String) {
    let hub = resolve_hub(arg_str(arguments, "hub").as_deref());
    match hub_get(&hub, "/status").await {
        Ok(t) => (true, pretty(&t)),
        Err(e) => (false, format!("hub_status 失败: {}", e)),
    }
}

/// host_report：GET /host-report → 本机资源探针记录，可选 limit 截断。
async fn run_host_report(arguments: &Map<String, JsonValue>) -> (bool, String) {
    let hub = resolve_hub(arg_str(arguments, "hub").as_deref());
    let limit = arg_int(arguments, "limit", 20);
    match hub_get(&hub, "/host-report").await {
        Ok(t) => match serde_json::from_str::<JsonValue>(&t) {
            Ok(mut v) => {
                if let Some(arr) = v.as_array_mut() {
                    arr.truncate(limit);
                }
                (true, pretty(&serde_json::to_string(&v).unwrap_or_default()))
            }
            Err(e) => (false, format!("host_report 响应不是合法 JSON: {}", e)),
        },
        Err(e) => (false, format!("host_report 失败: {}", e)),
    }
}

/// probe_server：单服探针，复用 motd::ping（Java 优先，回退 Bedrock）。
fn run_probe_server(arguments: &Map<String, JsonValue>) -> (bool, String) {
    let host = match required_str(arguments, "host") {
        Ok(v) => v,
        Err(e) => return (false, e.to_string()),
    };
    let port = args_port(arguments);
    let result = crate::motd::ping(&host, port, -1, true);
    if result.info.motd.is_empty() {
        return (
            false,
            format!("{}:{} 不可达（Java TCP 与 Bedrock UDP 均无响应）", host, port),
        );
    }
    let info = &result.info;
    let out = json!({
        "host": host,
        "port": port,
        "edition": format!("{:?}", result.protocol),
        "game_version": info.game_version,
        "protocol_version": info.protocol_version,
        "motd": info.motd,
        "players_online": info.players_online,
        "players_max": info.players_max,
        "players": info.players,
    });
    (true, pretty(&out.to_string()))
}

fn args_port(arguments: &Map<String, JsonValue>) -> u16 {
    arguments
        .get("port")
        .and_then(|v| v.as_u64())
        .map(|n| n as u16)
        .unwrap_or(25565)
}

fn pretty(text: &str) -> String {
    serde_json::from_str::<JsonValue>(text)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| text.to_string())
}

/// 在 stdio 上启动 MCP server 并等待退出。
/// 退出原因写到 stderr（MCP stdout 是协议通道，不污染）。
pub async fn serve_stdio(server: CreeperMcp) -> Result<(), Box<dyn std::error::Error>> {
    let serving = server.serve(stdio()).await?;
    let reason = serving.waiting().await?;
    eprintln!("[creeper-mcp] stdio server exited: {:?}", reason);
    Ok(())
}

/// main.rs 入口：启动 MCP server。
/// 默认 stdio；`http=true` 时起 HTTP（Streamable HTTP）监听，地址由 `addr` 指定。
pub async fn run(http: bool, addr: &str) -> anyhow::Result<()> {
    if http {
        eprintln!(
            "[creeper-mcp] http server starting (v{}).",
            env!("CARGO_PKG_VERSION")
        );
        serve_http(addr).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    } else {
        eprintln!(
            "[creeper-mcp] stdio server starting (v{}).",
            env!("CARGO_PKG_VERSION")
        );
        serve_stdio(CreeperMcp::new())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    Ok(())
}

/// 在 HTTP（Streamable HTTP，MCP 官方 transport）上启动 MCP server。
/// 仅显式 `creeper mcp --http` 时调用——默认不监听、不立服务。
/// 挂载路径 `/mcp`；复用项目已有的 axum 栈。
pub async fn serve_http(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
    use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};

    // StreamableHttpServerConfig 是 #[non_exhaustive]，外部 crate 不能用
    // 结构体字面量（含 `..` 更新），只能 Default 后按字段赋值。
    let mut config = StreamableHttpServerConfig::default();
    // 无会话 + 简单请求-响应直接回 JSON：initialize/tools/list/tools/call
    // 一次 POST 拿全响应，curl/脚本可直接调用（真实 MCP 客户端同样兼容）。
    config.json_response = true;
    config.legacy_session_mode = false;
    let service = StreamableHttpService::new(
        || Ok(CreeperMcp::new()),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let app = axum::Router::new().route_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!(
        "[creeper-mcp] http server listening on http://{addr}/mcp (Ctrl-C 停止)"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_cover_read_plane() {
        let s = CreeperMcp::new();
        let names: Vec<&str> = s.tools.iter().map(|t| t.name.as_ref()).collect();
        for core in ["hub_info", "journal", "servers", "hub_status", "host_report", "probe_server"] {
            assert!(names.contains(&core), "missing tool: {}", core);
        }
        // 攻击/抹盘/扫描发现/任务类不暴露
        for not in [
            "stress", "flood", "attack", "wipe", "discover", "scan", "crawl",
            "hub_submit", "hub_worker", "backup",
        ] {
            assert!(!names.contains(&not), "should not expose: {}", not);
        }
    }

    #[test]
    fn probe_schema_marks_host_required() {
        let s = CreeperMcp::new();
        let probe = s
            .tools
            .iter()
            .find(|t| t.name.as_ref() == "probe_server")
            .unwrap();
        let schema = probe.schema_as_json_value();
        assert_eq!(schema["type"], "object");
        assert!(schema["required"].as_array().unwrap().contains(&json!("host")));
        assert_eq!(schema["properties"]["host"]["type"], "string");
        assert_eq!(schema["properties"]["port"]["type"], "integer");
    }

    #[test]
    fn arg_helpers_read_values() {
        let mut m = Map::new();
        m.insert("host".into(), JsonValue::String("play.example.com".into()));
        m.insert("port".into(), JsonValue::Number(25565.into()));
        m.insert("limit".into(), JsonValue::Number(5.into()));
        assert_eq!(required_str(&m, "host").unwrap(), "play.example.com");
        assert_eq!(arg_int(&m, "limit", 100), 5);
        assert_eq!(arg_int(&m, "missing", 100), 100);
        assert_eq!(args_port(&m), 25565);
        assert!(required_str(&m, "missing").is_err());
    }

    #[test]
    fn resolve_hub_falls_back() {
        // 无显式参数、config 无 hub 时兜底 localhost:9090
        let hub = resolve_hub(None);
        assert!(!hub.is_empty());
        // 显式参数优先
        assert_eq!(resolve_hub(Some("http://10.0.0.5:9090")), "http://10.0.0.5:9090");
        // 空白参数视为未提供
        assert!(!resolve_hub(Some("  ")).is_empty());
    }
}
