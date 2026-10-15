# MCP 服务器接入指南（stdio + http）

> `creeper mcp` 启动 MCP server，把 creeper 的**只读数据面**与**单服探针**
> 暴露为 MCP 工具，供豆包连接器 / AtomCode / Claude / Cursor 等 MCP 客户端接入调用。
>
> 两种 transport：**默认 stdio**（进程内会话）；**HTTP**（Streamable HTTP，
> 仅显式 `--http` 才起监听，**默认不监听、不立服务**）。
> 实现：`src/mcp.rs`，基于 `rmcp` 3.1.2（**锁定版本**，见下方踩坑）。

## 1. 工具集（第一批：只读数据面 + 单服探针，不含攻击类）

| 工具 | 参数 | 作用 |
|---|---|---|
| `hub_info` | `hub?` | 查询 hub 节点元信息与端点列表（GET /） |
| `journal` | `hub?`、`limit?` | 拉取 hub 全部已发现服务器 + 玩家名册（GET /journal） |
| `servers` | `hub?`、`host?` | 服务器元数据 MOTD/版本/在线人数（GET /servers），可按地址子串过滤 |
| `hub_status` | `hub?` | hub 健康检查与统计（GET /status） |
| `host_report` | `hub?`、`limit?` | hub 上的本机资源探针记录（GET /host-report） |
| `probe_server` | `host`、`port?` | 单次探一个 Minecraft 服务器（Java 优先，回退 Bedrock；只读，不攻击） |

- **hub URL 解析**：显式 `hub` 参数 → 本地 config 默认节点（`hub.default_node` /
  `hub.nodes[0]` / 旧 `hub.url`）→ 兜底 `http://localhost:9090`。
- **设计边界（硬规则）**：MCP **不暴露**攻击类（stress / flood / attack 链）、
  扫描发现类（discover / scan / crawl）、抹盘类（wipe）、任务下发类
  （hub submit / worker）。这些保持 CLI 人工操作，不让 Agent 半自动调用。
- 每次工具调用追加审计到 `~/.cache/creeper/mcp-audit.jsonl`（who/what/ok/elapsed）。

## 2. 接入豆包桌面端（STDIO 连接器）

「新建/编辑自定义连接器」→ 传输方式选 **STDIO**。`mcp` 子命令**必须出现在
命令行里**，两种写法等价（连接器把命令+参数拼成命令行执行）：

- 写法 A：**命令**填 `creeper mcp`（空格形式，参数留空）；
- 写法 B：**命令**填 `/Users/user2/.cargo/bin/creeper`（或 `creeper`，需在 PATH），
  **参数**填 `mcp`。

两种都行；**漏掉 `mcp` 才会失败**（拉起无参 `creeper`，见踩坑 1）。推荐写法 B：
绝对路径不依赖 PATH，`mcp` 独立成参数更清晰。

保存后连接器应显示运行中；`pgrep -f "creeper mcp"` 可见子进程。

## 3. 接入 AtomCode / Claude / Cursor（.mcp.json）

```json
{
  "mcpServers": {
    "creeper": {
      "command": "/Users/user2/.cargo/bin/creeper",
      "args": ["mcp"],
      "autoApprove": ["hub_info", "journal", "servers", "hub_status", "host_report"]
    }
  }
}
```

- `autoApprove`：全部工具只读，可放行；`probe_server` 会探测外部服务器，保持询问亦可。
- 工具以 `mcp__<server键名>__<远端tool名>` 形式注册（如 `mcp__creeper__journal`）。
- 配置位置：`<项目根>/.mcp.json`（仅本项目）或 `~/.atomcode/mcp.json`（全局）。

## 4. HTTP transport（可选；默认不开监听、不立服务）

只有显式加 `--http` 才起 HTTP 服务，路径 `/mcp`：

```bash
creeper mcp --http --addr 127.0.0.1:3001   # http://127.0.0.1:3001/mcp
```

- 不加 `--http`：纯 stdio，**不监听任何端口**（`lsof -iTCP -sTCP:LISTEN` 无 creeper）。
- `--addr` 默认 `127.0.0.1:3001`，按需改；端口被占会报 `Address already in use`。
- 响应模式由客户端协商的协议自动决定：
  - 协商 `2026-07-28`（最新）→ **直接 JSON**（无会话，curl/脚本友好）；
  - 协商 legacy（2024-11-05 / 2025-03-26）→ SSE + `mcp-session-id` 会话。
- 新版协议下非 initialize 请求**必须带 SEP-2243 头**：`Mcp-Method`、
  有 name 的方法还需 `Mcp-Name`（RFC 2047 包装的 base64），body 带 `_meta`。

### HTTP curl 示例（2026-07-28 协议，直回 JSON）

```bash
BASE=http://127.0.0.1:3001/mcp
H='-H Content-Type:application/json -H "Accept:application/json, text/event-stream" -H MCP-Protocol-Version:2026-07-28'

# initialize
curl -s -X POST $BASE $H \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'

# tools/list（需 Mcp-Method 头 + _meta）
curl -s -X POST $BASE $H -H "Mcp-Method: tools/list" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}'

# tools/call（需 Mcp-Method + Mcp-Name，名称按 RFC 2047 包装 base64）
NAME=$(printf 'hub_status' | base64)
curl -s -X POST $BASE $H -H "Mcp-Method: tools/call" -H "Mcp-Name: =?base64?${NAME}?=" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hub_status","arguments":{"hub":"http://localhost:9090"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}'
```

## 5. 手动验证（stdio 协议，向进程喂 JSON-RPC）

先构建 release（`cargo build --release`），然后：

```bash
# initialize 握手
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"manual","version":"0"}}}\n' | ./target/release/creeper mcp | head -c 400

# tools/list
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"manual","version":"0"}}}\n{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n' | ./target/release/creeper mcp | tail -c 600

# tools/call（探针示例；对真实 hub 可用 hub 参数指向 http://<host>:9090）
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"manual","version":"0"}}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"probe_server","arguments":{"host":"127.0.0.1"}}}\n' | timeout 20 ./target/release/creeper mcp
```

本地起 hub 后可直接验证 hub 工具：

```bash
./target/release/creeper hub-serve --port 9090 &   # 后台起 hub
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"manual","version":"0"}}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"hub_status","arguments":{"hub":"http://localhost:9090"}}}\n' | timeout 20 ./target/release/creeper mcp
```

## 6. 踩坑记录（对抗上下文压缩）

1. **`mcp` 必须出现在命令行里（命令或参数，二选一）**：豆包连接器把「命令+参数」
   拼成命令行执行，命令填 `creeper mcp`（空格形式）与命令 `creeper`+参数 `mcp`
   等价；**两者都漏**（只填了命令没带 `mcp`）时，拉起的是无参数的 `creeper`
   （进 help/交互），JSON-RPC 握手无响应 → 连接器「运行失败」。
2. **rmcp 3.4.0 stdio 有 bug：initialize 响应后立即 `task cancelled`，tools/list 无响应**。
   降级到 **rmcp 3.1.2** 并手动实现 `ServerHandler`（`get_info`/`list_tools`/`call_tool`，
   参考 `rust-verb-shell/crates/rvs-mcp`）后正常。本项目 Cargo.toml 已锁定
   `rmcp = "=3.1.2"`，不要升回 3.4.0。
3. **日志污染 stdout 协议流（连接器「连接失败」的真实根因）**：`main()` 必须**最先分支**
   `Commands::Mcp`（任何日志初始化 / stdout 输出之前）。rmcp 内部 INFO 日志
   （`Service initialized as server` 等）一旦被 env_logger 写 stdout，客户端按行解析
   NDJSON 失败 → 握手失败。验证：`printf '…initialize…\n…tools/list…\n' | creeper mcp`
   的 stdout 每行都必须能被 `json.loads` 解析、且无 INFO/WARN 行。
4. **EOF 行为**：客户端关闭 stdin 后 server 结束 serve 循环（`input stream terminated`），
   属正常。
5. **hub URL 不依赖 cwd**：MCP 连接器从自己的工作目录拉起子进程（非项目根），
   config 相对路径找不到时工具用兜底 `http://localhost:9090`；连真实 hub 时
   请显式传 `hub` 参数。
6. **HTTP `Accept` 必须同时声明两种类型**：只发 `Accept: application/json` 会被
   406 拒绝（rmcp 要求客户端同时接受 `application/json` 与 `text/event-stream`）。
7. **新版协议（2026-07-28）下 curl 直调要带 SEP-2243 头**：非 initialize 请求缺
   `Mcp-Method` 报 `-32020 missing required Mcp-Method header`；`tools/call` 的
   `Mcp-Name` 需 `=?base64?<b64>?=` 形式，裸 base64 报「does not match body value」；
   body 的 `_meta` 需带 `protocolVersion` 与 `clientCapabilities`。
8. **HTTP 默认不开监听**：忘加 `--http` 就是纯 stdio，客户端用 http URL 连不上；
   端口占用报 `Address already in use (os error 48)`——换 `--addr` 或先停旧进程。
