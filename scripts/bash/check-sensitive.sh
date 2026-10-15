#!/usr/bin/env bash
#
# check-sensitive.sh — 提交/推送前敏感信息守卫
#
# 扫描即将入库的内容，拦截四类敏感信息（命中任一项即 exit 1 阻断）：
#   1. 公网 IPv4 / IPv6 地址（私网、回环、链路本地、文档段 一律放行）
#   2. SSH 密钥文件名（id_ed25519_*、id_rsa*、id_ecdsa* 等）
#   3. 私钥内容特征（BEGIN *PRIVATE KEY 头）
#   4. 疑似私人邮箱（放行占位符 example.com / 官方 trailer noreply@*）
#
# 用法:
#   scripts/bash/check-sensitive.sh              # 扫描暂存区（pre-commit 钩子用）
#   scripts/bash/check-sensitive.sh --push       # 扫描全部跟踪文件（pre-push / CI 用）
#   scripts/bash/check-sensitive.sh <file...>    # 显式指定文件
#
# 命中时输出违规文件:行:内容，替换为占位符（<your-ip> / <your-key> / <example@example.com>）
# 后重新提交。详见 AGENTS.md「脱敏与公开仓库政策」。

set -u

mode="staged"
args=()
for a in "$@"; do
  case "$a" in
    --push)          mode="push" ;;
    --staged)        mode="staged" ;;
    -h|--help)       sed -n '1,30p' "$0"; exit 0 ;;
    *)               args+=("$a") ;;
  esac
done

# ── 收集要扫描的文件 ─────────────────────────────────────────────
files=()
if [[ ${#args[@]} -gt 0 ]]; then
  files=("${args[@]}")
elif [[ "$mode" == "push" ]]; then
  # pre-push / CI：扫全部被跟踪文件（git ls-files 只列当前树，不含历史）
  while IFS= read -r f; do files+=("$f"); done < <(git ls-files)
else
  # pre-commit：扫暂存区（新增/修改/复制）
  while IFS= read -r f; do files+=("$f"); done < <(git diff --cached --name-only --diff-filter=ACM)
fi
[[ ${#files[@]} -eq 0 ]] && exit 0

# ── 豁免清单：政策/守卫自身/生成物文件 ──────────────────────────────
# 这些文件引用密钥文件名、IP 等词是必要的「规则描述」，不是真实资产：
#   AGENTS.md                    脱敏政策（禁例示例）
#   scripts/bash/check-sensitive.sh / hooks/pre-commit   守卫自身（正则与说明）
#   Cargo.lock                   生成锁文件；crate 版本号（如 1.3.268.0）
#                               会被 IPv4 正则误判，且内容完全由 Cargo.toml 决定
exempt_files=("AGENTS.md" "scripts/bash/check-sensitive.sh" "hooks/pre-commit" "Cargo.lock")
filtered=()
for f in "${files[@]}"; do
  skip=0
  for e in "${exempt_files[@]}"; do
    [[ "$f" == "$e" ]] && { skip=1; break; }
    # 目录级豁免：条目以 / 结尾时按前缀匹配（如 vendor/ 豁免整个第三方目录）
    [[ "$e" == */ && "$f" == "$e"* ]] && { skip=1; break; }
  done
  (( skip == 0 )) && filtered+=("$f")
done
# macOS 自带 bash 3.2 下 `set -u` 对空数组展开会报 unbound variable，
# 先判空再展开（暂存区全为豁免文件时 filtered 为空）。
if (( ${#filtered[@]} > 0 )); then
  files=("${filtered[@]}")
else
  files=()
fi
[[ ${#files[@]} -eq 0 ]] && exit 0

# ── IPv4 私网/保留段判定（这些不是公网资产，放行）──────────────────
is_private_v4() {
  local a b c d
  IFS=. read -r a b c d <<< "$1"
  (( a == 0 )) && return 0            # 0/8 默认路由等
  (( a == 10 )) && return 0           # 10/8 私网
  (( a == 100 && b >= 64 && b <= 127 )) && return 0   # 100.64/10 CGNAT
  (( a == 127 )) && return 0          # 回环
  (( a == 169 && b == 254 )) && return 0             # 链路本地
  (( a == 172 && b >= 16 && b <= 31 )) && return 0   # 172.16/12 私网
  (( a == 192 && b == 0 && (c == 0 || c == 2) )) && return 0  # 192.0.0/24 192.0.2/24
  (( a == 192 && b == 88 && c == 99 )) && return 0   # 192.88.99/24
  (( a == 192 && b == 168 )) && return 0             # 192.168/16 私网
  (( a == 198 && (b == 18 || b == 19) )) && return 0 # 198.18/15 基准测试
  (( a == 198 && b == 51 && c == 100 )) && return 0  # 198.51.100/24 TEST-NET-2
  (( a == 203 && b == 0 && c == 113 )) && return 0   # 203.0.113/24 TEST-NET-3
  (( a >= 224 )) && return 0          # 组播/保留
  return 1                            # 其余视为公网
}

# ── IPv6 非公网段判定 ────────────────────────────────────────────
is_private_v6() {
  local addr="$1"
  [[ "$addr" == "::" || "$addr" == "::1" ]] && return 0
  [[ "$addr" =~ ^fe[89ab] ]] && return 0      # fe80::/10 链路本地
  [[ "$addr" =~ ^f[cd] ]] && return 0         # fc00::/7 ULA
  [[ "$addr" =~ ^2001:db8 ]] && return 0      # 2001:db8::/32 文档段
  return 1                                    # 其余视为公网
}

violations=0

report() {  # $1=file  $2=reason  $3=line-content
  printf '  %s:%s\n' "$1" "$3"
  violations=$((violations + 1))
}

for f in "${files[@]}"; do
  [[ -f "$f" ]] || continue

  # 1) 公网 IPv4
  while IFS= read -r hit; do
    n="${hit%%:*}"; content="${hit#*:}"
    for ip in $(grep -IoE '[0-9]{1,3}(\.[0-9]{1,3}){3}' <<< "$content"); do
      if ! is_private_v4 "$ip"; then
        report "$f" "公网IPv4" "$n: $content"
      fi
    done
  done < <(grep -InE '[0-9]{1,3}(\.[0-9]{1,3}){3}' "$f" 2>/dev/null)

  # 2) 公网 IPv6（全形式，如 2001:19f0:...）
  while IFS= read -r hit; do
    n="${hit%%:*}"; content="${hit#*:}"
    for addr in $(grep -IoE '[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){3,7}' <<< "$content"); do
      if ! is_private_v6 "$addr"; then
        report "$f" "公网IPv6" "$n: $content"
      fi
    done
  done < <(grep -InE '[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){3,7}' "$f" 2>/dev/null)

  # 3) SSH 密钥文件名
  while IFS= read -r hit; do
    report "$f" "密钥文件名" "$hit"
  done < <(grep -InE 'id_(ed25519|rsa|ecdsa|dsa|ed448)[A-Za-z0-9_.-]*' "$f" 2>/dev/null)

  # 4) 私钥内容头
  while IFS= read -r hit; do
    report "$f" "私钥内容" "$hit"
  done < <(grep -InE 'BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY' "$f" 2>/dev/null)

  # 5) 疑似私人邮箱（放行占位符与 trailer；放行 SSH URL 形式 git@host:path）
  while IFS= read -r hit; do
    case "$hit" in
      *example.com*|*example.org*|*example.net*|*noreply@*) ;;
      *)
        # SSH 克隆地址 `git@host:path` 不是邮箱（git@ 后跟域名+冒号路径）
        if [[ "$hit" =~ git@[A-Za-z0-9.-]+\.[A-Za-z]{2,}: ]]; then
          continue
        fi
        report "$f" "邮箱" "$hit" ;;
    esac
  done < <(grep -InE '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}' "$f" 2>/dev/null)

  # 6) 机器别名 / 云平台名 / 型号黑名单
  # 用户要求：连「有几台机器、什么平台」都不能暴露。以下词是真实机器标识，
  # 出现即说明把机器清单写进了公开仓库，必须换成 <lan-host> / <vps> / <your-key> 占位符。
  # 注意：只列「独特词」——arch/ubuntu 等通用词不拦（可能误伤正常语义）。
  while IFS= read -r hit; do
    report "$f" "机器标识" "$hit"
  done < <(grep -InEi 'printer' "$f" 2>/dev/null)
done

if (( violations > 0 )); then
  echo "❌ 敏感信息拦截：检测到 $violations 处可能泄露（文件:行:内容）"
  echo "   请替换为占位符后重新提交：<your-ip> / <your-key> / <example@example.com>"
  exit 1
fi
exit 0
