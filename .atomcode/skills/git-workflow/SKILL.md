---
name: git-workflow
description: 每次代码变更后自动执行 git 提交+推送 — git status → 分组 → commit → push → 循环
user_invocable: false
---

# Git 自动提交流程

每次完成代码编辑后，**主动执行以下流程**，不需要用户确认。

commit message 格式使用以下标签标明所处阶段：

| 标签 | 含义 |
|------|------|
| `[AI]` | AI 提交的代码变更 |
| `[AI尝试]` | AI 尝试实现某个功能（可能未完成） |
| `[AI通过]` | AI 通过了编译/测试验证 |
| `[人类测试]` | 人类手动测试后确认通过 |
| `[人类修正]` | 人类手动修正了 AI 的代码 |

## 流程

### 第 1 步：查看变更
```bash
git status
```

### 第 2 步：检查 .gitignore
如有明显不应跟踪的文件（编译产物、日志、凭据等），追加到 `.gitignore` 后一并提交。

### 第 3 步：按功能分组提交
将变更按功能分组，对每一组：
```bash
git add <文件1> <文件2> ...
git commit -m "[AI] <type>: <简短描述>"
```

Commit message 格式：`[AI] feat/fix/refactor/chore: 描述`

末尾自动添加：
```

Co-Authored-By: AtomCode (deepseek-v4-flash) <noreply@atomgit.com>
```

### 第 4 步：循环
回到 `git status`，直到没有变更。

### 第 5 步：推送
```bash
git push
```
