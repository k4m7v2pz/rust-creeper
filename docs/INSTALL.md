# 安装教程：creeper 二进制装在哪、怎么接 MCP

> 一句话版：二进制装在 **`~/.cargo/bin/creeper`**（`cargo install` 常驻副本）；
> 接 MCP 就是「**二进制路径 + `mcp` 子命令**」两个字段。

## 1. 二进制安装位置

```plaintext
~/.cargo/bin/creeper
```

cargo 的标准安装目录（`~/.cargo/bin` 在 PATH 里），安装命令：

```bash
cd ~/Documents/Code.localized/rust-creeper
cargo install --path .          # 构建并装到 ~/.cargo/bin/creeper
```

### 为什么是常驻副本，而不是指向 target/release 的软链或绝对路径

`creeper` 的构建产物默认在 `target/release/creeper`。**不要**在客户端配置里写
这个 target 路径，也不要依赖指向它的软链——因为**人类会定期清理 rust 项目的
`target/`**，清理后这些引用全部失效。

`~/.cargo/bin` 的常驻副本与 `target/` 无关：清完 `target/`，`creeper` 照常可用。

### PATH 优先级说明

`~/.cargo/bin` 在 PATH 中位于 `/usr/local/bin` 之前。装好后：

```bash
which creeper    # 应输出 /Users/user2/.cargo/bin/creeper
```

若机器上另有 `/usr/local/bin/creeper`（旧软链等），`creeper` 命令会优先解析到
常驻副本，不受影响；旧软链在清 target 后可能成死链，可自行清理。

## 2. 更新到新版本

常驻副本**不会**跟随 rebuild 自动更新（这是它相对软链的代价），源码改动后需重装：

```bash
cargo install --path . --force   # --force 覆盖同名旧版本
creeper --version                # 确认新版本号
```

## 3. 验证安装

```bash
which creeper                     # ~/.cargo/bin/creeper
creeper --version                 # creeper 2.4.0
creeper mcp --help                # 应有 --http / --addr 选项
```

## 4. 接 MCP：二进制路径 + mcp 子命令

MCP 客户端（豆包连接器 / AtomCode / Claude / Cursor）的配置本质只有两个字段：
**命令 = 二进制路径，参数 = `mcp`**。

### STDIO（默认，客户端拉起子进程，不占端口）

豆包桌面端「自定义连接器」→ STDIO。`mcp` 子命令**必须出现在命令行里**，两种写法等价：

- **写法 A**：命令填 `creeper mcp`（空格形式，参数留空）
- **写法 B**：命令填 `/Users/user2/.cargo/bin/creeper`（或 `creeper`，需在 PATH），参数填 `mcp`

推荐写法 B：绝对路径不依赖 PATH，`mcp` 独立成参数更清晰。

AtomCode / Claude / Cursor 的 `.mcp.json`：

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

### HTTP（可选；**默认不开监听、不立服务**，只有显式 `--http` 才起）

```bash
creeper mcp --http --addr 127.0.0.1:3001
```

- 不加 `--http`：纯 stdio，不监听任何端口（`lsof -iTCP -sTCP:LISTEN` 无 creeper）。
- `--addr` 默认 `127.0.0.1:3001`，按需修改；端口被占报 `Address already in use`。
- HTTP 端点：`http://127.0.0.1:3001/mcp`
- 响应模式按客户端协商的协议自动决定：协商 `2026-07-28` → 直接 JSON；
  协商 legacy（2024-11-05 / 2025-03-26）→ SSE + 会话。curl 直调示例见
  `docs/MCP.md` 第 4 节（新版协议需带 SEP-2243 头）。

## 5. 常见问题

| 现象 | 原因与处理 |
|---|---|
| 连接器「运行失败」 | 命令行里漏了 `mcp`（命令和参数二选一放即可）；补上再试 |
| HTTP 连不上 | 忘了加 `--http`（默认纯 stdio 不监听）；加上再连 |
| `Address already in use` | 端口被占；换 `--addr` 端口或先停旧进程 |
| 版本对不上（`--version` 旧） | 源码改动后没重装；`cargo install --path . --force` |

## 6. 相关文档

- 工具集、客户端配置、踩坑记录：`docs/MCP.md`
- 部署到远端：`deploy/deploy.sh`（`REMOTE_HOST` 必填，真实主机信息在本地
  `secrets/remote-hosts.md`，不入库）
