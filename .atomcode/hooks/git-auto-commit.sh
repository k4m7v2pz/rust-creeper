#!/bin/bash
# git-workflow hook — 代码变更后自动提交推送
# 由 .atomcode/settings.json 的 post_tool_use 触发

set -e

PROJECT_DIR="$1"
cd "$PROJECT_DIR"

# 检查是否有变更
if git diff --quiet && git diff --cached --quiet && [[ -z $(git ls-files --others --exclude-standard) ]]; then
    exit 0
fi

# 暂存所有变更（包括新增文件）
git add -A

# 生成提交信息：汇总变更文件
CHANGED=$(git diff --cached --name-only | head -5)
COUNT=$(git diff --cached --name-only | wc -l | tr -d ' ')
if [ "$COUNT" -le 5 ]; then
    FILES=$(echo "$CHANGED" | tr '\n' ' ')
    MSG="[AI] auto: ${FILES}"
else
    FIRST=$(echo "$CHANGED" | head -3 | tr '\n' ' ')
    MSG="[AI] auto: ${FIRST}等${COUNT}个文件"
fi

# 截断过长的消息
if [ ${#MSG} -gt 100 ]; then
    MSG="${MSG:0:97}..."
fi

git commit -m "$MSG

Co-Authored-By: AtomCode (deepseek-v4-flash) <noreply@atomgit.com>"

# 推送
git push 2>&1 || echo "推送失败（可能是无远程或网络问题）"
