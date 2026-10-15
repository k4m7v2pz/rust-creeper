# MCP 服务器接入指南（stdio）

> `creeper mcp` 启动一个 **stdio MCP server**，把 creeper 的**只读数据面**与**单服探针**
> 暴露为 MCP 工具，供豆包连接器 / AtomCode / Claude / Cursor 等 MCP 客户端接入调用。
>
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

「新建/编辑自定义连接器」→ 传输方式选 **STDIO**：

- **命令**：`/Users/user2/Documents/Code.localized/rust-creeper/target/release/creeper`
- **参数**：点「+ 添加」填 `mcp`（**必须填参数**，漏掉表现为连接器「运行失败」，见踩坑 1）

保存后连接器应显示运行中；`pgrep -f "creeper mcp"` 可见子进程。

## 3. 接入 AtomCode / Claude / Cursor（.mcp.json）

```json
{
  "mcpServers": {
    "creeper": {
      "command": "/Users/user2/Documents/Code.localized/rust-creeper/target/release/creeper",
      "args": ["mcp"],
      "autoApprove": ["hub_info", "journal", "servers", "hub_status", "host_report"]
    }
  }
}
```

- `autoApprove`：全部工具只读，可放行；`probe_server` 会探测外部服务器，保持询问亦可。
- 工具以 `mcp__<server键名>__<远端tool名>` 形式注册（如 `mcp__creeper__journal`）。
- 配置位置：`<项目根>/.mcp.json`（仅本项目）或 `~/.atomcode/mcp.json`（全局）。

## 4. 手动验证（stdio 协议，向进程喂 JSON-RPC）

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

## 5. 踩坑记录（对抗上下文压缩）

1. **STDIO 连接器必须显式传 `mcp` 参数**：豆包连接器填了命令但没填参数，拉起的是
   无参数的 `creeper`（进 help/交互），JSON-RPC 握手无响应 → 连接器「运行失败」。
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
