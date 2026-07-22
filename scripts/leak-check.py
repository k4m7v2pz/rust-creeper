#!/usr/bin/env python3
"""
leak-check.py — 提交前敏感信息扫描

扫描 git 暂存区中的新增/修改行，检查是否泄漏敏感信息。
零依赖，仅用 Python 标准库。

用法：
    uv run python scripts/leak-check.py          # 扫描暂存区
    uv run python scripts/leak-check.py --full   # 扫描整个工作区（含未暂存文件）
    uv run python scripts/leak-check.py --help   # 帮助

返回码：
    0 = 无泄漏
    1 = 发现泄漏（需修复后重新 add + commit）
"""
import re
import subprocess
import sys
from pathlib import Path

# ── 泄漏模式 ──────────────────────────────────────────────────────────
# 每条规则: (模式名, 正则, 是否允许 192.168/10.x/172.16 内网 IP)
# 注意: 按 severity 降序排列，越敏感的越靠前

LEAK_PATTERNS = [
    # ── 严重级别 ──
    ("SSH_PRIVATE_KEY", re.compile(
        r'-----BEGIN\s+(?:RSA|DSA|EC|OPENSSH|PRIVATE)\s+KEY-----'
    ), False),
    ("PASSWORD_OR_TOKEN", re.compile(
        r'(?i)(?:password|passwd|pwd|secret|token|api_key|apikey|auth_token)'
        r'\s*[=:]\s*[\'"][^\'"]+[\'"]'
    ), False),
    ("REAL_EMAIL", re.compile(
        r'[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'
    ), False),

    # ── 中等级别 ──
    ("PUBLIC_IP", re.compile(
        r'\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b'
    ), True),  # allow_private=True means 192.168/10.x/172.16 are OK
    ("LOCAL_ABSOLUTE_PATH", re.compile(
        r'/(?:Users|home|root|tmp|opt|var|etc)/(?:[^/\s]*/)*[^/\s]'
    ), False),
    ("PROXY_PORT_NUMBER", re.compile(
        r'\b(?:789[0-9]|108[0-9]| socks[45]://127\.0\.0\.1:\d+)\b'
    ), False),

    # ── 低级别 ──
    ("SSH_HOSTNAME_OR_IP", re.compile(
        r'(?i)(?:ssh|host|HostName)[=:]\s*(?:\d{1,3}\.){3}\d{1,3}'
    ), False),
    ("PRIVATE_SSH_KEY_PATH", re.compile(
        r'(?i)(?:IdentityFile|ssh\s+-i)\s+~?/.*/(?:id_rsa|id_ed25519|id_ecdsa)'
    ), False),
    ("PASSWORD_IN_SSH", re.compile(
        r"sshpass\s+-p\s+'[^']+'"
    ), False),
]

# 内网 IP 段（允许出现在公开仓库中）
PRIVATE_IP_PREFIXES = ('10.', '172.16.', '172.17.', '172.18.', '172.19.',
                       '172.20.', '172.21.', '172.22.', '172.23.', '172.24.',
                       '172.25.', '172.26.', '172.27.', '172.28.', '172.29.',
                       '172.30.', '172.31.', '192.168.')

# 放行模式（白名单正则）
ALLOW_PATTERNS = [
    # 占位符
    re.compile(r'<[^>]+@[^>]+\.[^>]+>'),  # <example@example.com>
    re.compile(r'<[^>]+>'),               # <proxy-port>, <your-remote>, etc.
    re.compile(r'127\.0\.0\.1'),           # localhost
    re.compile(r'0\.0\.0\.0'),             # 任意地址
    re.compile(r'::1'),                    # IPv6 localhost
    # 代码中的通用占位/示例
    re.compile(r'example\.(?:com|org|net)'),
    re.compile(r'username|password|your_token|your_key|your_password',
               re.IGNORECASE),
    # 文档中的代码示例
    re.compile(r'`[^`]*`'),  # 行内代码块内容由肉眼判断
    # 项目架构路径（非本地个人路径）
    re.compile(r'/opt/creeper/'),
    re.compile(r'/etc/systemd/system/'),
]


def is_private_ip(ip: str) -> bool:
    """判断是否为内网 IP"""
    return any(ip.startswith(prefix) for prefix in PRIVATE_IP_PREFIXES)


def is_allowed(line: str) -> bool:
    """检查是否被白名单放行"""
    for pattern in ALLOW_PATTERNS:
        if pattern.search(line):
            return True
    return False


def scan_staged_files(full_mode: bool = False) -> int:
    """扫描暂存区（或整个工作区）的泄漏"""
    if full_mode:
        # 扫描整个工作区未提交的文件
        result = subprocess.run(
            ['git', 'ls-files', '--others', '--modified', '--exclude-standard'],
            capture_output=True, text=True, timeout=30
        )
        files = [f for f in result.stdout.strip().split('\n') if f]
    else:
        # 扫描暂存区
        result = subprocess.run(
            ['git', 'diff', '--cached', '--name-only'],
            capture_output=True, text=True, timeout=30
        )
        files = [f for f in result.stdout.strip().split('\n') if f]

    if not files:
        print("[leak-check] 没有待检查的文件（暂存区为空）")
        return 0

    # 排除二进制/图片/锁定文件
    BINARY_EXTENSIONS = {'.png', '.jpg', '.jpeg', '.gif', '.ico', '.pdf',
                         '.woff', '.woff2', '.ttf', '.eot', '.o', '.so', '.dylib'}
    SKIP_FILES = {'Cargo.lock', 'pnpm-lock.yaml', 'package-lock.json', 'yarn.lock'}

    findings = []
    seen = set()

    for filepath in files:
        filepath = filepath.strip()
        if not filepath:
            continue
        ext = Path(filepath).suffix.lower()
        if ext in BINARY_EXTENSIONS or filepath in SKIP_FILES:
            continue
        if not Path(filepath).exists():
            continue

        # 获取新增/修改的行（diff 中 + 号开头的行）
        if full_mode:
            with open(filepath, 'r', errors='ignore') as f:
                lines = f.readlines()
            staged_lines = [(i+1, line) for i, line in enumerate(lines)]
        else:
            diff_result = subprocess.run(
                ['git', 'diff', '--cached', '-U0', '--', filepath],
                capture_output=True, text=True, timeout=30
            )
            staged_lines = []
            for dline in diff_result.stdout.split('\n'):
                if dline.startswith('+') and not dline.startswith('+++'):
                    staged_lines.append((0, dline[1:]))

        for lineno, line in staged_lines:
            if is_allowed(line):
                continue
            for name, pattern, allow_private in LEAK_PATTERNS:
                for match in pattern.finditer(line):
                    matched_text = match.group()
                    # 对 IP 地址的特殊处理
                    if name == "PUBLIC_IP" and allow_private:
                        ip = matched_text
                        if is_private_ip(ip):
                            continue
                        # 127.0.0.1 也在白名单中
                        if ip == '127.0.0.1':
                            continue
                    # 对 email 的特殊处理：放行 noreply@ 和占位符
                    if name == "REAL_EMAIL":
                        if 'noreply@' in matched_text or '<' in matched_text:
                            continue

                    key = (filepath, name, matched_text[:60])
                    if key not in seen:
                        seen.add(key)
                        findings.append((filepath, lineno, name, matched_text))

    if findings:
        print("[leak-check] ⚠️  发现潜在敏感信息泄漏！\n")
        for filepath, lineno, name, text in findings:
            loc = f":{lineno}" if lineno else ""
            print(f"  [{name}] {filepath}{loc}")
            snippet = text[:80] + ('...' if len(text) > 80 else '')
            print(f"           └─ {snippet}")
        print()
        print("  处理方式：")
        print("  1. 是占位符/示例（如 <example@example.com>、192.168.x.x、<public-ip>）→ 放行")
        print("  2. 是真实数据 → git restore --staged <file> 摘出，替换为占位符后重新 git add")
        print("  3. 或是误报 → 在 ALLOW_PATTERNS 或脚本中增加白名单规则")
        print()
        return 1
    else:
        print("[leak-check] ✅ 未发现敏感信息泄漏")
        return 0


def main():
    full_mode = False
    if '--help' in sys.argv or '-h' in sys.argv:
        print(__doc__)
        return 0
    if '--full' in sys.argv:
        full_mode = True

    try:
        # 检查是否在 git 仓库中
        subprocess.run(['git', 'rev-parse', '--git-dir'],
                       capture_output=True, check=True, timeout=10)
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("[leak-check] ❌ 不在 git 仓库中")
        return 1

    return scan_staged_files(full_mode)


if __name__ == '__main__':
    sys.exit(main())