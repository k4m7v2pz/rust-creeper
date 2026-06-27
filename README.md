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
* Supports SOCKS4/5 + HTTP proxies
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
| `-v`, `--version`    | Minecraft version. Default `1.21.1`                                                |
| `-r`, `--register`   | Auto /register + /login on join                                                    |
| `--help`             | Print help                                                                         |

Full usage guide: [`docs/usage.md`](docs/usage.md)

## License

**Unlicense** — see [LICENSE](LICENSE).  
This project is released into the public domain.

Attribution for referenced projects (MIT / Apache 2.0) is in [`reference/README.md`](reference/README.md).
