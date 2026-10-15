#!/usr/bin/env bash
# deploy.sh — Build creeper for Linux (musl), push to remote host, install services
set -euo pipefail

# 真实主机名/用户名只本地传参（如 REMOTE_HOST=xxx REMOTE_USER=yyy bash deploy/deploy.sh），
# 不入库；未显式指定时拒绝部署（防止误推到占位/默认目标）。
REMOTE_HOST="${REMOTE_HOST:?未设置 REMOTE_HOST —— 真实主机名只本地传参，不入库}"
REMOTE_USER="${REMOTE_USER:-creeper}"
REMOTE_DIR="/opt/creeper"

echo "=== 1. Building release binary (x86_64-unknown-linux-musl) ==="
cargo build --release --target x86_64-unknown-linux-musl 2>&1
BIN="target/x86_64-unknown-linux-musl/release/creeper"
if [[ ! -f "$BIN" ]]; then
    echo "ERROR: musl build failed — trying gnu target"
    cargo build --release --target x86_64-unknown-linux-gnu 2>&1
    BIN="target/x86_64-unknown-linux-gnu/release/creeper"
    if [[ ! -f "$BIN" ]]; then
        echo "FATAL: No Linux binary found. Install musl or gnu target."
        exit 1
    fi
fi
echo "Binary: $BIN ($(du -h "$BIN" | cut -f1))"

echo ""
echo "=== 2. Creating archive ==="
TMPDIR=$(mktemp -d)
cp "$BIN" "$TMPDIR/creeper"
cp data/targets-template.json "$TMPDIR/"
cp deploy/creeper-hub.service "$TMPDIR/"
cp deploy/creeper-monitor.service "$TMPDIR/"
chmod +x "$TMPDIR/creeper"

echo ""
echo "=== 3. Pushing to remote ($REMOTE_HOST) ==="
ssh "${REMOTE_USER}@${REMOTE_HOST}" "mkdir -p ${REMOTE_DIR}/data"
scp "$TMPDIR/creeper" "${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_DIR}/"
scp "$TMPDIR/targets-template.json" "${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_DIR}/data/mc-targets.json"

echo ""
echo "=== 4. Installing systemd services ==="
scp "$TMPDIR/creeper-hub.service" "${REMOTE_USER}@${REMOTE_HOST}:/tmp/"
scp "$TMPDIR/creeper-monitor.service" "${REMOTE_USER}@${REMOTE_HOST}:/tmp/"
ssh "${REMOTE_USER}@${REMOTE_HOST}" "
    sudo mv /tmp/creeper-hub.service /etc/systemd/system/
    sudo mv /tmp/creeper-monitor.service /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable --now creeper-hub
    sudo systemctl enable --now creeper-monitor
"

echo ""
echo "=== 5. Checking service status ==="
ssh "${REMOTE_USER}@${REMOTE_HOST}" "
    echo '--- creeper-hub ---'
    sudo systemctl status creeper-hub --no-pager -l || true
    echo ''
    echo '--- creeper-monitor ---'
    sudo systemctl status creeper-monitor --no-pager -l || true
"

echo ""
echo "=== Deploy complete ==="
rm -rf "$TMPDIR"
