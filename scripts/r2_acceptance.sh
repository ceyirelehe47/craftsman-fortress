#!/usr/bin/env bash
# R2 单一验收入口：先重跑 R1 图形基线，再执行可修改世界与存档往返/损坏恢复。
set -euo pipefail
cd "$(dirname "$0")/.."

EVIDENCE_DIR="${1:-evidence_r2_$(date +%Y%m%d_%H%M%S)}"

if [ -n "$(git status --porcelain)" ]; then
  echo "失败：工作区必须干净。" >&2
  git status --porcelain >&2
  exit 1
fi
if [ -e "$EVIDENCE_DIR" ] && [ -n "$(ls -A "$EVIDENCE_DIR" 2>/dev/null)" ]; then
  echo "失败：证据目录已存在且非空：$EVIDENCE_DIR" >&2
  exit 1
fi
mkdir -p "$EVIDENCE_DIR"
if ! git check-ignore -q "$EVIDENCE_DIR"; then
  echo "失败：R2 证据目录必须被 .gitignore 忽略，否则嵌套的 R1 验收会把它视为脏工作区：$EVIDENCE_DIR" >&2
  exit 1
fi
SHA=$(git rev-parse HEAD)
printf '%s\n' "$SHA" > "$EVIDENCE_DIR/commit.txt"

cleanup() {
  local code=$?
  if [ "$code" -ne 0 ] && [ -d "$EVIDENCE_DIR" ]; then
    printf '%s\n' "$code" > "$EVIDENCE_DIR/last_exit_code.txt" || true
  fi
}
trap cleanup EXIT

step() { printf '\n===== %s =====\n' "$1"; }

step "R2 static checks"
bash -n scripts/*.sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
bash scripts/audit_bevy.sh
bash scripts/check_naming.sh
cargo test --release
cargo build --release

step "R1 regression acceptance"
ACCEPTANCE_WATCHDOG_SEC="${ACCEPTANCE_WATCHDOG_SEC:-1200}" \
  bash scripts/acceptance.sh "$EVIDENCE_DIR/r1"
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$EVIDENCE_DIR/r1/report.json" \
  || { echo "R1 report is not PASS" >&2; exit 1; }

step "R2 persistence roundtrip"
mkdir -p "$EVIDENCE_DIR/r2"
cargo run --release --example r2_roundtrip -- "$EVIDENCE_DIR/r2" \
  > "$EVIDENCE_DIR/r2_stdout.log" 2>&1
cat "$EVIDENCE_DIR/r2_stdout.log"
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$EVIDENCE_DIR/r2/report.json" \
  || { echo "R2 report is not PASS" >&2; exit 1; }

cat > "$EVIDENCE_DIR/report.json" <<JSON
{
  "commit": "$SHA",
  "r1": "PASS",
  "r2": "PASS",
  "overall": "PASS",
  "r1_report": "r1/report.json",
  "r2_report": "r2/report.json"
}
JSON
cat > "$EVIDENCE_DIR/report.md" <<MD
# R2 aggregate acceptance

- commit: \`$SHA\`
- R1 graphical baseline: **PASS**
- R2 edit/save/load/corruption recovery: **PASS**
- overall: **PASS**
MD

find "$EVIDENCE_DIR" -type f ! -name SHA256SUMS.txt -print0 \
  | xargs -0 sha256sum > "$EVIDENCE_DIR/SHA256SUMS.txt"
ZIP_FILE="${EVIDENCE_DIR}.zip"
rm -f "$ZIP_FILE"
if command -v zip >/dev/null 2>&1; then
  zip -qr "$ZIP_FILE" "$EVIDENCE_DIR"
elif command -v powershell >/dev/null 2>&1; then
  powershell -NoProfile -Command \
    "Compress-Archive -Path '$EVIDENCE_DIR' -DestinationPath '$ZIP_FILE'" >/dev/null
fi
[ -f "$ZIP_FILE" ] && echo "evidence package: $ZIP_FILE"
echo "R2 acceptance PASS: $EVIDENCE_DIR"
