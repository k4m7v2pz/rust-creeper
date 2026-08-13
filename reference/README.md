# Reference Projects

此目录用于存放**各类仓库**（自己的或他人的），作为本项目的参考资料。

## 使用方式

在 `reference/` 目录下执行 `git clone` 来添加参考仓库：

```bash
cd reference
git clone <另一个仓库的URL>
```

## 目录结构

```
reference/
├── README.md             # ✅ 提交（说明文档+保留目录）
└── <其他仓库>/           # ❌ 不提交（通过 git clone 添加）
```

## 注意事项

- ⚠️ 所有子目录都会被 `.gitignore` 忽略，不会提交到 Git
- ✅ `README.md` 会保留在仓库中
- 💡 每次使用 `git clone` 后，这些仓库不会影响主项目的 Git 状态

### .gitignore 配置

项目根目录的 `.gitignore` 中关于 `reference/` 的配置：

```gitignore
reference/*
!reference/README.md
```

## 当前参考仓库

| 项目 | 许可证 | 说明 |
|---|---|---|
| [mcprotocollib](mcprotocollib/) | MIT | GeyserMC/MCProtocolLib — Java Minecraft 协议库 |
| [mc-bots-ref](mc-bots-ref/) | MIT | crpmax/mc-bots — Java Minecraft 机器人压力测试 |
| [holy-client-ref](holy-client-ref/) | Apache 2.0 | Titlehhhh/Minecraft-Holy-Client — C# 高性能压力测试机器人 |
| [endmc-ref](endmc-ref/) | — | SerendipityR-2022/EndMinecraftPlusV2 — Java Minecraft 压力测试，支持 Forge 握手 + 反作弊绕过 |
| [motd-stress-ref](motd-stress-ref/) | MIT | konsheng/MinecraftMotdStressTest — Python MOTD 压测工具（mcstatus + ThreadPoolExecutor） |

## License Attributions

### MIT — games647/LambdaAttack (original Java project)

This Rust port is based on the original Java project
[games647/LambdaAttack](https://github.com/games647/LambdaAttack),
which is licensed under the MIT License.
完整版权声明与许可证全文见根目录 [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md)。

### MIT — GeyserMC/MCProtocolLib

Embedded at `mcprotocollib/`. Copyright (c) 2021-2024 GeyserMC. See `mcprotocollib/LICENSE.txt`.

### MIT — crpmax/mc-bots

Embedded at `mc-bots-ref/`. Copyright (c) 2021 CreeperMaxCZ. See `mc-bots-ref/LICENSE`.

### Apache 2.0 — Minecraft-Holy-Client

Embedded at `holy-client-ref/`. See `holy-client-ref/LICENSE.txt`.

### MIT — konsheng/MinecraftMotdStressTest

Embedded at `motd-stress-ref/`. Copyright (c) 2025~2099 Konsheng. See `motd-stress-ref/LICENSE`.
