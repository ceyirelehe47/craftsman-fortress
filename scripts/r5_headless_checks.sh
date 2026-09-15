#!/usr/bin/env bash
# R5 无窗口验收：建筑权威层、结构规则、跨层约束与 CFBLD001 三层配对恢复。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

OUT="${1:-/tmp/craftsman-r5-headless}"
WATCHDOG_SEC="${R5_HEADLESS_WATCHDOG_SEC:-300}"
if [ -e "$OUT" ] && [ -n "$(ls -A "$OUT" 2>/dev/null)" ]; then
  echo "失败：R5 headless 输出目录必须为空：$OUT" >&2
  exit 1
fi
mkdir -p "$OUT"
SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT

cargo build --release --example r5_building_roundtrip
BIN="target/release/examples/r5_building_roundtrip"
[ -x "$BIN.exe" ] && BIN="$BIN.exe"

"$BIN" "$OUT" >"$SCRATCH/stdout.log" 2>&1 &
pid=$!
deadline=$(( $(date +%s) + WATCHDOG_SEC ))
while kill -0 "$pid" 2>/dev/null; do
  if [ "$(date +%s)" -ge "$deadline" ]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    echo "R5 building roundtrip exceeded ${WATCHDOG_SEC}s" > "$OUT/watchdog.txt"
    tail -n 100 "$SCRATCH/stdout.log" >> "$OUT/watchdog.txt" 2>/dev/null || true
    exit 124
  fi
  sleep 1
done
code=0
wait "$pid" || code=$?
cp "$SCRATCH/stdout.log" "$OUT/app_stdout.log"
[ "$code" -eq 0 ] || { cat "$OUT/app_stdout.log" >&2; exit "$code"; }

REPORT="$OUT/report.json"
[ -s "$REPORT" ] || { echo "失败：R5 headless 缺少 report.json" >&2; exit 1; }
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$REPORT" || {
  cat "$REPORT" >&2
  exit 1
}
for id in F01 F02 F03 F04 F05 F06 F07 F08 F09 F10 F11 F12; do
  grep -Fq "\"id\":\"$id\"" "$REPORT" || {
    echo "失败：R5 headless 缺少 $id。" >&2
    exit 1
  }
done
hits=$(grep -nE 'panic|ERROR|FATAL|B0004' "$OUT/app_stdout.log" || true)
[ -z "$hits" ] || { printf '%s\n' "$hits" >&2; exit 1; }

find "$OUT" -type f ! -name SHA256SUMS.txt -print0 | xargs -0 sha256sum > "$OUT/SHA256SUMS.txt"
echo "R5 headless PASS: $OUT"
