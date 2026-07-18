# AGENTS.md — Agent 协作规约

本文件是本仓库的 Agent 协作约定。**Agent 在与人类的协作方式发生变动时，必须自动编辑本文件以反映现状。** 人类也可随时手动修订。

---

## 一、脱敏与公开仓库政策

> **本仓库为公开仓库，提交到 git 的内容默认会被互联网可见。**

### 1. 提交内容脱敏

凡进入 git 的内容（代码、文档、commit message、注释、配置）必须满足：

- **不得包含** 个人邮箱、真实姓名、私钥、token、密码、私人服务器地址、代理端口（如本机 socks / http 复用端口）、内部 IP、SSH 口令。
- **不得包含** 未公开的私人仓库地址。公开开源仓库地址可保留。
- **commit message** 里不要嵌入远端 URL、不要嵌入用户私人邮箱；trailer 统一用 `Co-Authored-By: AtomCode (GLM-5.2) <noreply@atomgit.com>`。
- **文档里** 若要举例远端、邮箱、端口，用占位符（`<example@example.com>`、`<proxy-port>`、`<your-remote>`、`<ssh-port>`）。

### 2. 用 .gitignore 忽略不该进库的本地数据

Agent 在提交前必须核对暂存区，下列内容**不得入库**，应写入 `.gitignore`：

| 类别 | 示例 | 理由 |
|---|---|---|
| 监控目标 / 真实数据 | `data/mc-targets.json` 含真实服主域名端口 | 非项目交付，敏感 |
| 本地缓存 / 构建产物 | `target/`、`.DS_Store`、`*.log` | 体积大、机器相关 |
| 私人笔记 / 草稿 | `notes/private.md`、`scratch/` | 个人用，非项目交付 |
| 第三方克隆参考仓 | `reference/<其他仓>` | 旁路参考，非本仓代码 |
| IDE 本地配置 | `.idea/`、`.vscode/`（除非团队共享） | 机器相关 |
| 部署目标清单 | `deploy/*.local.sh`、`hosts.txt` | 含私服地址 |

### 3. 提交前核对流程

Agent 在执行 `git commit` 前必须：

1. `git status --short` + `git diff --cached --name-only` 列暂存区
2. 肉眼扫一遍：有无 `data/mc-targets.json`、私人邮箱、token、本地绝对路径、私服 IP/SSH 口令泄漏
3. 若有误网，`git restore --staged <file>` 摘出，必要时加进 `.gitignore`
4. 确认无泄漏再 commit

---

## 二、Agent 自动提交与推送

### 1. 何时自动提交

当人类明确要求"提交并推送"、"你来处理提交"等时，Agent 可直接执行 `git add` → `git commit` → `git push`，无需每步停下问人。

### 2. commit message 规范

- 首行：`<type>: <概要>`，type 用 `feat` / `fix` / `docs` / `refactor` / `chore` / `test`
- 空行后正文：要点列表，说明做了什么、为什么
- 末尾 trailer（空行隔开）：
  ```
  Co-Authored-By: AtomCode (deepseek-v4-flash) <noreply@atomgit.com>
  ```
- 用 `git commit -m "$(cat <<'EOF' ... EOF)"` heredoc 保空行；`--amend` / `revert` 不加 trailer

### 3. 推送前确认

- 推送前 `git log -1 --format='%B'` 校验 message 完整（trailer 不应裸成首行）
- 推送目标分支默认当前分支（`git push origin <current>`），不擅自改远端或新建分支
- 推送失败不重试同一命令，先读错误（权限 / 非快进 / 拒接）再修

---

## 三、协作方式自维护

**触发条件**：Agent 与人类的协作方式发生变动时，例如：

- 人类指定了新的代理或网络配置（本机 socks/http 复用端口等）→ 不要写进 git，但要在本文件"附录"里记协作约束
- 人类偏好变更（如"部署脚本不得留明文 SSH 口令"）→ 在"附录"里记设计原则
- 新的自动行为约定（如"交叉编译产物必须 `cargo check` 过才能 scp"）→ 在本文件里记成规则
- 工具链锁定（如"musl target 锁定 x86_64-unknown-linux-musl"）→ 在"附录"里记技术锚点

**执行方式**：Agent 在执行完变动后，`edit_file` 本文件追加/修订对应条目，下次会话 Agent 读到本文件即继承约定。

---

## 四、附录：本项目当前约定

> 本节是 Agent 维护的动态部分，记录与本项目具体协作约定。

### A. 技术锚点

- **Rust + cargo**：本仓是 Rust 项目，依赖见 `Cargo.toml`。
- **musl target 锁 `x86_64-unknown-linux-musl`**：交叉编译远端 Linux 二进制用此 target；不要混用 gnu target。
- **`.cargo/config.toml` 已配链接器**：不要手动改链接器配置，除非 musl-cross 工具链升级。

### B. 网络与代理

- **本机代理服务**：HTTP 复用端口与 SOCKS5 复用端口由本机代理服务提供（具体端口值不入 git，用 `<proxy-http-port>` / `<proxy-socks-port>` 占位）。rustup / cargo 如遇网络问题（下载超时、连接失败）可走代理：
  ```bash
  # HTTP 代理
  export https_proxy=http://127.0.0.1:<proxy-http-port>
  export http_proxy=http://127.0.0.1:<proxy-http-port>
  cargo build --release

  # 或 SOCKS5 代理
  export ALL_PROXY=socks5://127.0.0.1:<proxy-socks-port>
  rustup target add x86_64-unknown-linux-musl
  ```
- **github.com 走代理**：克隆/拉取 github 远端时走代理。`gitcode.com` / 国内远端走直连。
- Agent 克隆仓库时按远端域名判断是否走代理，**不要把代理端口写进任何提交内容**。

### C. 交叉编译与部署

- **交叉编译步骤**（Mac → Linux 远端）：
  1. `rustup target add x86_64-unknown-linux-musl`
  2. `brew install FiloSottile/musl-cross/musl-cross`
  3. `.cargo/config.toml` 已配置链接器
  4. `cargo build --release --target x86_64-unknown-linux-musl`
- **部署参考 `DEPLOY.md`**，或直接执行 `bash deploy/deploy.sh`（先把 `ARCH_HOST` 改成占位远端）。
- **部署脱敏**：部署脚本、systemd unit、配置文件里不得留明文 SSH 口令、私服真实 IP。真实远端信息只存本地不入库。

### D. 设计原则

- **探针 vs 扫描节点分离**：承担"探针"角色（仅监控在线/资源占用）的远端**不得**派发端口扫描/对外攻击任务（`discover`/`scan`/`crawl`/`http`/`start`）。部署时只装 `serve`+`monitor`，**不装 worker**（worker 会从 hub poll 并执行 Scan/Discover 任务）。
- **Hub 多连接方式**：`config.json` 的 `hub.nodes` 支持每个 hub 登记多个连接方式（IPv4 直连 / IPv6 直连 / 内网穿透端口 / 正向反向 HTTP/WS 等），按 `priority` 自动选可用 URL。详见 `src/config.rs` 的 `HubNode` / `HubConnection`。
- **"有库优先用库"**：优先用现有 crate（reqwest / axum / sysinfo / clap），实在不行才自己写。

### E. 工作流锚点

- **改动验证**：每次改动后 `cargo check` 验证编译，`cargo test` 验证测试，不要跳过。
- **不引入重依赖**：如非必要不拉大依赖；schema 校验改用轻量手写 + 静态 schema 供外部工具。
- **commit 前核对暂存区**：按本文件"脱敏政策"第 3 条执行。
- **远端部署前**：交叉编译产物在本地 `cargo check --target x86_64-unknown-linux-musl` 过再 scp；systemd unit 装好后 `systemctl status` 验证不报错才算部署完成。
