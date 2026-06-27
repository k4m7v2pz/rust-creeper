# LambdaAttack (Rust)

Minecraft stress-test bot.  
**Rust port** of [games647/LambdaAttack](https://github.com/games647/LambdaAttack).

## Features

* Command line interface
* Configurable amount and join delay
* Configurable target
* Configurable name format or name list
* Test with Spigot, Paper
* Disconnects gracefully after the end
* Automatically registers for cracked servers
* Supports SOCKS proxies
* Free
* Open source

## Requirements

* Rust 1.75+
* Minecraft 1.21.1+ server (for the current protocol implementation)

## Building

```bash
cargo build --release
cargo test
```

## Usage

```
cargo run --release -- [OPTIONS]
```

| Flag                 | Description                                                                        |
|----------------------|------------------------------------------------------------------------------------|
| `-h`, `--host`       | Server hostname. Default `127.0.0.1`                                               |
| `-p`, `--port`       | Server port. Default `25565`                                                       |
| `-c`, `--count`      | Number of bots. Default `20`                                                       |
| `-d`, `--delay`      | Spawn delay in milliseconds. Default `1000`                                        |
| `-n`, `--name`       | Bot name format (requires `%d`). Default `Bot-%d`                                  |
| `-v`, `--version`    | Minecraft version. Default `1.15.2`                                                |
| `-r`, `--register`   | Auto /register + /login on join                                                    |
| `--help`             | Print help                                                                         |

## Project Structure

```
├── Cargo.toml                  # Workspace root
├── protocol/                   # Protocol traits (UniversalProtocol, GameVersion, etc.)
├── protocol-v1_21_1/           # 1.21.1 protocol implementation
└── core/                       # Orchestration logic (CLI, bot spawning, attack)
```

## License

**Unlicense** — see [LICENSE](LICENSE).  
This project is released into the public domain.

### Attribution (MIT — original Java project)

This Rust port is based on the original Java project  
[games647/LambdaAttack](https://github.com/games647/LambdaAttack),  
which is licensed under the MIT License:

> The MIT License (MIT)
> Copyright (c) 2016
> 
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
> 
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
> 
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

### Embedded Reference Implementations

参考仓库统一放在 [`reference/`](reference/)，详见 [`reference/README.md`](reference/README.md)。

| 项目 | 许可证 | 目录 | 说明 |
|---|---|---|---|
| [GeyserMC/MCProtocolLib](https://github.com/GeyserMC/MCProtocolLib) | MIT | `reference/mcprotocollib/` | Java Minecraft 协议库（1.11.2 ~ 1.21.7 tags） |
| [crpmax/mc-bots](https://github.com/crpmax/mc-bots) | MIT | `reference/mc-bots-ref/` | Java Minecraft 机器人压力测试 |
| [Titlehhhh/Minecraft-Holy-Client](https://github.com/Titlehhhh/Minecraft-Holy-Client) | Apache 2.0 | `reference/holy-client-ref/` | C# 高性能压力测试机器人 |

### Friends

- [MCProtocolLib](https://github.com/Steveice10/MCProtocolLib) — Original Minecraft protocol library (MIT)
- [azalea](https://github.com/azalea-rs/azalea) — Rust Minecraft bot framework
