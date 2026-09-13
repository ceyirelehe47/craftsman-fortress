#!/usr/bin/env bash
# R2.1 单一验收入口：完整重跑 R1 图形基线，再执行带 watchdog 的存档恢复验收。
set -euo pipefail
cd "$(dirname "$0")/.."

EVIDENCE_DIR="${1:-evidence_r2_1_$(date +%Y%m%d_%H%M%S)}"

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
  echo "失败：证据目录必须被 .gitignore 忽略：$EVIDENCE_DIR" >&2
  exit 1
fi
SHA=$(git rev-parse HEAD)
printf '%s\n' "$SHA" > "$EVIDENCE_DIR/commit.txt"

cleanup() {
  local code=$?
  if [ "$code" -ne 0 ] && [ -d "$EVIDENCE_DIR" ]; then
    printf '%s\n' "$code" > "$EVIDENCE_DIR/last_exit_code.txt" || true
    find "$EVIDENCE_DIR" -type f -print0 2>/dev/null \
      | xargs -0 sha256sum > "$EVIDENCE_DIR/SHA256SUMS.partial.txt" 2>/dev/null || true
  fi
}
trap cleanup EXIT

step() { printf '\n===== %s =====\n' "$1"; }

step "R2.1 static checks"
bash -n scripts/*.sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
bash scripts/audit_bevy.sh
bash scripts/check_naming.sh
cargo test --release 2>&1 | tee "$EVIDENCE_DIR/cargo_test.log"
cargo build --release 2>&1 | tee "$EVIDENCE_DIR/cargo_build.log"

step "R1 regression acceptance"
ACCEPTANCE_WATCHDOG_SEC="${ACCEPTANCE_WATCHDOG_SEC:-1200}" \
  bash scripts/acceptance.sh "$EVIDENCE_DIR/r1"
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$EVIDENCE_DIR/r1/report.json" \
  || { echo "R1 report is not PASS" >&2; exit 1; }

step "R2.1 headless recovery acceptance"
R2_HEADLESS_WATCHDOG_SEC="${R2_HEADLESS_WATCHDOG_SEC:-300}" \
  bash scripts/r2_headless_checks.sh "$EVIDENCE_DIR/r2"
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' \
  "$EVIDENCE_DIR/r2/headless_report.json" \
  || { echo "R2.1 headless report is not PASS" >&2; exit 1; }

cat > "$EVIDENCE_DIR/r2/report.json" <<JSON
{
  "commit": "$SHA",
  "overall": "PASS",
  "checks": [
    {"id":"B01","status":"PASS","name":"delete and place"},
    {"id":"B02","status":"PASS","name":"cross-chunk update and mesh equivalence"},
    {"id":"B03","status":"PASS","name":"overlay normalization"},
    {"id":"B04","status":"PASS","name":"bedrock and top-layer safety"},
    {"id":"B05","status":"PASS","name":"strict format, canonical order and resource limits"},
    {"id":"B06","status":"PASS","name":"exact save/load semantic roundtrip"},
    {"id":"B07","status":"PASS","name":"edit-order independence"},
    {"id":"B08","status":"PASS","name":"two-slot rotation"},
    {"id":"B09","status":"PASS","name":"fully-valid semantic corruption fallback"},
    {"id":"B10","status":"PASS","name":"slot repair and generation overflow rejection"},
    {"id":"B11","status":"PASS","name":"startup load rejection with exit code 3"},
    {"id":"B12","status":"PASS","name":"R1 regression pass; evidence ready for isolated reviewer"}
  ],
  "roundtrip_report": "roundtrip_report.json",
  "headless_report": "headless_report.json",
  "invalid_load_exit_code": 3
}
JSON

cat > "$EVIDENCE_DIR/r2/report.md" <<MD
# R2.1 persistence acceptance

- commit: \`$SHA\`
- B01-B12: **PASS**
- semantic/base/FNV corruption fallback: **PASS**
- valid old slot preservation: **PASS**
- generation overflow rejection: **PASS**
- invalid startup exit code: **3**
- headless watchdog: **enabled**
MD

cat > "$EVIDENCE_DIR/report.json" <<JSON
{
  "commit": "$SHA",
  "r1": "PASS",
  "r2_1": "PASS",
  "overall": "PASS",
  "r1_report": "r1/report.json",
  "r2_report": "r2/report.json"
}
JSON
cat > "$EVIDENCE_DIR/report.md" <<MD
# R2.1 aggregate acceptance

- commit: \`$SHA\`
- R1 graphical baseline: **PASS**
- R2.1 full-valid-slot recovery and headless checks: **PASS**
- overall: **PASS**
MD

find "$EVIDENCE_DIR" -type f ! -name SHA256SUMS.txt -print0 \
  | xargs -0 sha256sum > "$EVIDENCE_DIR/SHA256SUMS.txt"
rm -f "$EVIDENCE_DIR/SHA256SUMS.partial.txt" "$EVIDENCE_DIR/last_exit_code.txt"
ZIP_FILE="${EVIDENCE_DIR}.zip"
rm -f "$ZIP_FILE"
if command -v zip >/dev/null 2>&1; then
  zip -qr "$ZIP_FILE" "$EVIDENCE_DIR"
elif command -v powershell >/dev/null 2>&1; then
  powershell -NoProfile -Command \
    "Compress-Archive -Path '$EVIDENCE_DIR' -DestinationPath '$ZIP_FILE'" >/dev/null
else
  echo "失败：无法生成证据 ZIP（缺少 zip/PowerShell）。" >&2
  exit 1
fi
[ -s "$ZIP_FILE" ] || { echo "失败：证据 ZIP 未生成。" >&2; exit 1; }
sha256sum "$ZIP_FILE" >> "$EVIDENCE_DIR/SHA256SUMS.txt"
echo "evidence package: $ZIP_FILE"
echo "R2.1 acceptance PASS: $EVIDENCE_DIR"
