#!/usr/bin/env bash
# R4 无窗口对象权威层、伴随存档与崩溃回退验收。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

OUT="${1:-evidence_r4_headless_$(date +%Y%m%d_%H%M%S)}"
WATCHDOG_SEC="${R4_HEADLESS_WATCHDOG_SEC:-300}"
if [ -e "$OUT" ] && [ -n "$(ls -A "$OUT" 2>/dev/null)" ]; then
  echo "失败：R4 headless 输出目录已存在且非空：$OUT" >&2
  exit 1
fi
mkdir -p "$OUT"
SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT

cargo build --release --example r4_object_roundtrip
BIN="target/release/examples/r4_object_roundtrip"
[ -x "$BIN.exe" ] && BIN="$BIN.exe"
"$BIN" "$OUT" > "$SCRATCH/stdout.log" 2>&1 &
pid=$!
deadline=$(( $(date +%s) + WATCHDOG_SEC ))
while kill -0 "$pid" 2>/dev/null; do
  if [ "$(date +%s)" -ge "$deadline" ]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    echo "R4 object roundtrip exceeded ${WATCHDOG_SEC}s" > "$OUT/watchdog.txt"
    tail -n 100 "$SCRATCH/stdout.log" >> "$OUT/watchdog.txt" 2>/dev/null || true
    exit 124
  fi
  sleep 1
done
code=0
wait "$pid" || code=$?
cp "$SCRATCH/stdout.log" "$OUT/app_stdout.log"
[ "$code" -eq 0 ] || { cat "$OUT/app_stdout.log" >&2; exit "$code"; }
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$OUT/report.json" || {
  echo "失败：R4 headless report 不是 PASS。" >&2; exit 1;
}
for id in E01 E02 E03 E04 E05 E06 E07 E08 E09 E10 E11 E12; do
  grep -Fq "\"id\":\"$id\"" "$OUT/report.json" || {
    echo "失败：R4 headless report 缺少 $id。" >&2; exit 1;
  }
done
printf '%s\n' "$(git rev-parse HEAD)" > "$OUT/commit.txt"
echo "R4 headless PASS: $OUT"
