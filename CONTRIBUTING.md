# Contributing to Creeper

感谢参与本项目!以下是贡献前必须了解的规则。

## 数据安全(强制)

本仓库为**公开仓库**,严禁将任何真实/敏感数据提交到 git。包括但不限于:

- **真实 IP 地址** — 无论内网(`192.168.x.x`、`10.x.x.x`、`172.16-31.x.x`)还是公网 IP,
  文档/代码/配置/日志/注释里均不得出现真实值。改用占位符:
  - `<your-hub-ip>`、`<target-ip>` 等显式占位
  - RFC 5737 测试地址:`192.0.2.0/24`(TEST-NET-1)、`198.51.100.0/24`(TEST-NET-2)、`203.0.113.0/24`(TEST-NET-3)
  - `127.0.0.1`、`0.0.0.0` 仅用于本机/监听示例,可保留
- **真实域名** — 尤其是被监控/扫描/攻击目标的域名,严禁入库。改用 `example.com`、`mc.example.com` 等 RFC 2606 保留域名
- **玩家名、UUID、聊天记录、IP 历史、备注** 等社工程序采集到的真实数据 — 严禁入库
- **真实凭证** — 密码、token、私钥、含账密的代理 URL(`socks5://user:pass@host`)
- **本机/内网拓扑信息** — 真实主机名、内网 IP 段、服务端口映射等

### 提交前自查

```bash
# 1. 检查暂存区是否含疑似真实 IP(排除保留/占位地址)
git diff --cached | grep -E '\b([0-9]{1,3}\.){3}[0-9]{1,3}\b' \
  | grep -vE '127\.0\.0\.1|0\.0\.0\.0|198\.51\.100|203\.0\.113|192\.0\.2|255\.255\.255|1\.1\.1\.1|8\.8\.8\.8'

# 2. 检查暂存区是否含疑似真实域名(排除保留域名)
git diff --cached | grep -iE '[a-z0-9-]+\.(com|net|org|cn|io|xyz|top)' \
  | grep -vE 'example\.com|example\.net|example\.org|crates\.io|github\.com|rust-lang\.org'
```

上述两条命令若有输出,需逐行核对是否为真实数据,确认无害后方可提交。

### 例外

- `src/http_flood.rs` 里 Cloudflare 官方 IP 段列表 — 属于公开 CIDR 参考,非目标数据,允许保留
- `Cargo.lock` 里的 crate 源 URL — 公开依赖信息,允许保留
- 公共 DNS(`1.1.1.1`、`8.8.8.8`)在文档中作为示例 — 允许保留

## 开发流程

1. Fork → 新建分支 → 提交 → 发 PR
2. 提交信息用中文,简洁描述改动
3. `cargo build --release` 通过后再提交

```bash
cargo build --release
cargo clippy -- -D warnings  # 可选,但推荐
```

## 代码风格

- Rust 标准风格,`cargo fmt` 格式化
- 中文注释 OK,新代码优先用英文标识符,保留已有中文命名约定
- 公开 API 加 doc comment
