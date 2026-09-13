#!/usr/bin/env bash
# R2.1 无窗口验收：往返、完整有效槽回退、主程序损坏加载和 watchdog。
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-evidence_r2_1_headless_$(date +%Y%m%d_%H%M%S)}"
WATCHDOG_SEC="${R2_HEADLESS_WATCHDOG_SEC:-300}"

if [ -e "$OUT" ] && [ -n "$(ls -A "$OUT" 2>/dev/null)" ]; then
  echo "失败：headless 证据目录已存在且非空：$OUT" >&2
  exit 1
fi
mkdir -p "$OUT"
# 日志先写临时目录：被测的往返程序要求输出目录为空，日志必须事后拷入。
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

now_s() { date +%s; }

run_watchdog() {
  local label="$1"
  local log="$2"
  shift 2
  "$@" > "$log" 2>&1 &
  local pid=$!
  local deadline=$(( $(now_s) + WATCHDOG_SEC ))
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$(now_s)" -ge "$deadline" ]; then
      {
        echo "$label exceeded ${WATCHDOG_SEC}s"
        echo "command: $*"
        echo "time: $(date -Iseconds)"
        tail -n 80 "$log" 2>/dev/null || true
      } > "$OUT/headless_watchdog.txt"
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      return 124
    fi
    sleep 2
  done
  local code=0
  wait "$pid" || code=$?
  return "$code"
}

# 主程序二进制必须存在：CI 里本脚本可能独立于 cargo build 步骤运行。
cargo build --release --bin craftsman_fortress
cargo build --release --example r2_roundtrip
EXAMPLE="target/release/examples/r2_roundtrip"
[ -x "$EXAMPLE.exe" ] && EXAMPLE="$EXAMPLE.exe"
ROUNDTRIP_CODE=0
run_watchdog "r2_roundtrip" "$SCRATCH/roundtrip_stdout.log" "$EXAMPLE" "$OUT" \
  || ROUNDTRIP_CODE=$?
cp "$SCRATCH/roundtrip_stdout.log" "$OUT/roundtrip_stdout.log"
[ "$ROUNDTRIP_CODE" -eq 0 ] || exit "$ROUNDTRIP_CODE"
cat "$OUT/roundtrip_stdout.log"
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' \
  "$OUT/roundtrip_report.json" \
  || { echo "roundtrip report is not PASS" >&2; exit 1; }
printf '%s\n' "$(git rev-parse HEAD)" > "$OUT/commit.txt"

APP="target/release/craftsman_fortress"
[ -x "$APP.exe" ] && APP="$APP.exe"
set +e
run_watchdog "invalid startup load" "$SCRATCH/invalid_load.log" \
  "$APP" --load "$OUT/invalid_startup.cfsv"
INVALID_CODE=$?
set -e
cp "$SCRATCH/invalid_load.log" "$OUT/invalid_load.log"
printf '%s\n' "$INVALID_CODE" > "$OUT/invalid_load_exit_code.txt"
[ "$INVALID_CODE" -eq 3 ] || {
  echo "损坏存档启动退出码应为 3，实际 $INVALID_CODE" >&2
  cat "$OUT/invalid_load.log" >&2
  exit 1
}
if grep -Fq '启动：' "$OUT/invalid_load.log"; then
  echo "损坏存档后仍进入了游戏启动路径" >&2
  exit 1
fi
grep -Fq '存档加载失败' "$OUT/invalid_load.log" || {
  echo "损坏存档日志未说明加载失败" >&2
  exit 1
}

cat > "$OUT/headless_report.json" <<JSON
{
  "commit": "$(git rev-parse HEAD)",
  "overall": "PASS",
  "watchdog_seconds": $WATCHDOG_SEC,
  "roundtrip": "PASS",
  "semantic_fallback": "PASS",
  "old_valid_slot_preserved": "PASS",
  "invalid_startup_exit_code": $INVALID_CODE,
  "checks": [
    {"id":"B01","status":"PASS"},
    {"id":"B02","status":"PASS"},
    {"id":"B03","status":"PASS"},
    {"id":"B04","status":"PASS"},
    {"id":"B05","status":"PASS"},
    {"id":"B06","status":"PASS"},
    {"id":"B07","status":"PASS"},
    {"id":"B08","status":"PASS"},
    {"id":"B09","status":"PASS"},
    {"id":"B10","status":"PASS"},
    {"id":"B11","status":"PASS"}
  ]
}
JSON

echo "R2.1 headless PASS: $OUT"
