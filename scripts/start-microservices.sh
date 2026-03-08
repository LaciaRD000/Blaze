#!/usr/bin/env bash
set -euo pipefail

# Blaze Bot マイクロサービス起動スクリプト
# Usage: ./scripts/start-microservices.sh [worker_count]
#   worker_count: Worker プロセス数（デフォルト: 4）

WORKER_COUNT="${1:-4}"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PIDS=()

cleanup() {
    echo ""
    echo "シャットダウン中..."
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill -TERM "$pid"
        fi
    done
    wait
    echo "全プロセスが停止しました"
}

trap cleanup EXIT INT TERM

cd "$PROJECT_DIR"

# ビルド（全バイナリを一括）
echo "ビルド中..."
cargo build --release --bin blaze-gateway --bin blaze-worker

echo "=== Blaze Bot マイクロサービス ==="
echo "  Worker 数: ${WORKER_COUNT}"
echo ""

# Worker 起動
for i in $(seq 1 "$WORKER_COUNT"); do
    cargo run --release --bin blaze-worker &
    PIDS+=($!)
    echo "[Worker ${i}] PID=${PIDS[-1]}"
done

# Gateway 起動
cargo run --release --bin blaze-gateway &
PIDS+=($!)
echo "[Gateway] PID=${PIDS[-1]}"

echo ""
echo "全プロセス起動完了 (Ctrl+C で停止)"
wait
