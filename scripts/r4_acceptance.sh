#!/usr/bin/env bash
# R4 单一验收入口：R3.1 全回归 + 对象权威层/伴随存档 + PalmStudio 对象视觉双跑。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

EVIDENCE_DIR="${1:-evidence_r4_$(date +%Y%m%d_%H%M%S)}"
MANIFEST="${R3_MANIFEST:-assets/vendor_local/voxel_survival_pack/v1.0/free_sample/selection.tsv}"
WATCHDOG_SEC="${R4_VISUAL_WATCHDOG_SEC:-180}"

[ -z "$(git status --porcelain)" ] || {
  echo "失败：工作区必须干净。" >&2
  git status --porcelain >&2
  exit 1
}
if [ -e "$EVIDENCE_DIR" ] && [ -n "$(ls -A "$EVIDENCE_DIR" 2>/dev/null)" ]; then
  echo "失败：证据目录已存在且非空：$EVIDENCE_DIR" >&2
  exit 1
fi
mkdir -p "$EVIDENCE_DIR"
git check-ignore -q "$EVIDENCE_DIR" || {
  echo "失败：R4 证据目录必须被 .gitignore 忽略。" >&2; exit 1;
}
SHA=$(git rev-parse HEAD)
printf '%s\n' "$SHA" > "$EVIDENCE_DIR/commit.txt"
SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT

run_watchdog() {
  label="$1"; log="$2"; shift 2
  "$@" > "$log" 2>&1 &
  pid=$!
  deadline=$(( $(date +%s) + WATCHDOG_SEC ))
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$(date +%s)" -ge "$deadline" ]; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      {
        echo "$label exceeded ${WATCHDOG_SEC}s"
        echo "command: $*"
        tail -n 100 "$log" 2>/dev/null || true
      } > "$EVIDENCE_DIR/r4_watchdog.txt"
      return 124
    fi
    sleep 1
  done
  code=0
  wait "$pid" || code=$?
  return "$code"
}

printf '\n===== static checks =====\n'
bash -n scripts/*.sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release
cargo build --release
bash scripts/audit_bevy.sh
bash scripts/check_naming.sh
bash scripts/test_r3_source_chain.sh

printf '\n===== R3.1 full regression and private-cache acquisition =====\n'
R3_ASSET_CACHE_FILE="${R3_ASSET_CACHE_FILE:-}" \
ACCEPTANCE_WATCHDOG_SEC="${ACCEPTANCE_WATCHDOG_SEC:-1500}" \
R2_HEADLESS_WATCHDOG_SEC="${R2_HEADLESS_WATCHDOG_SEC:-300}" \
R3_ASSET_WATCHDOG_SEC="${R3_ASSET_WATCHDOG_SEC:-420}" \
  bash scripts/r3_1_accept_from_cache.sh "$EVIDENCE_DIR/r3_1"
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$EVIDENCE_DIR/r3_1/report.json" || {
  echo "失败：R3.1 回归不是 PASS。" >&2; exit 1;
}

printf '\n===== R4 headless object layer =====\n'
R4_HEADLESS_WATCHDOG_SEC="${R4_HEADLESS_WATCHDOG_SEC:-300}" \
  bash scripts/r4_headless_checks.sh "$EVIDENCE_DIR/headless"

printf '\n===== R4 actual-GLB visual lab A/B =====\n'
cargo build --release --example r4_object_lab
LAB="target/release/examples/r4_object_lab"
[ -x "$LAB.exe" ] && LAB="$LAB.exe"
for run in a b; do
  out="$EVIDENCE_DIR/lab_$run"
  code=0
  run_watchdog "r4_object_lab_$run" "$SCRATCH/lab_${run}.log" \
    "$LAB" "$MANIFEST" "$out" || code=$?
  mkdir -p "$out"
  cp "$SCRATCH/lab_${run}.log" "$out/app_stdout.log"
  [ "$code" -eq 0 ] || { cat "$out/app_stdout.log" >&2; exit "$code"; }
  grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$out/report.json" || {
    echo "失败：R4 lab_$run 不是 PASS。" >&2; exit 1;
  }
  hits=$(grep -nE 'panic|ERROR|FATAL|B0004' "$out/app_stdout.log" || true)
  [ -z "$hits" ] || { printf '%s\n' "$hits" >&2; exit 1; }
done
hash_a=$(grep -o '"object_hash":"[^"]*"' "$EVIDENCE_DIR/lab_a/report.json" | head -1 | cut -d'"' -f4)
hash_b=$(grep -o '"object_hash":"[^"]*"' "$EVIDENCE_DIR/lab_b/report.json" | head -1 | cut -d'"' -f4)
[ -n "$hash_a" ] && [ "$hash_a" = "$hash_b" ] || {
  echo "失败：R4 visual 双跑对象哈希不一致：$hash_a / $hash_b" >&2; exit 1;
}
for name in R4_01_object_lineup.png R4_02_rotated_footprints.png R4_03_management_view.png R4_04_voxel_context.png; do
  sha_a=$(sha256sum "$EVIDENCE_DIR/lab_a/screenshots/$name" | awk '{print $1}')
  sha_b=$(sha256sum "$EVIDENCE_DIR/lab_b/screenshots/$name" | awk '{print $1}')
  [ "$sha_a" = "$sha_b" ] || { echo "失败：截图双跑不一致：$name" >&2; exit 1; }
done

LOCAL_DIR=$(dirname "$MANIFEST")
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$LOCAL_DIR/inventory.tsv" \
  "$EVIDENCE_DIR"

cat > "$EVIDENCE_DIR/report.json" <<JSON
{
  "commit":"$SHA",
  "overall":"PASS",
  "r3_1":"PASS",
  "r4_headless":"PASS",
  "r4_visual":"PASS",
  "visual_object_hash":"$hash_a",
  "checks":[
    {"id":"E01","status":"PASS","name":"stable persistent object ids"},
    {"id":"E02","status":"PASS","name":"support clearance bounds and overlap rules"},
    {"id":"E03","status":"PASS","name":"quarter-turn footprint rotation"},
    {"id":"E04","status":"PASS","name":"terrain edits cannot invalidate objects"},
    {"id":"E05","status":"PASS","name":"select place move rotate delete semantics"},
    {"id":"E06","status":"PASS","name":"object semantic hash and insertion-order independence"},
    {"id":"E07","status":"PASS","name":"strict companion object format"},
    {"id":"E08","status":"PASS","name":"exact terrain plus object roundtrip"},
    {"id":"E09","status":"PASS","name":"corrupted newest object slot fallback"},
    {"id":"E10","status":"PASS","name":"protected old pair across interrupted terrain commit"},
    {"id":"E11","status":"PASS","name":"legacy terrain-only save loads empty object layer"},
    {"id":"E12","status":"PASS","name":"actual GLB mapping screenshots evidence and reviewer readiness"}
  ]
}
JSON
cat > "$EVIDENCE_DIR/report.md" <<MD
# R4 independent-object-layer acceptance

- commit: \`$SHA\`
- R3.1 full regression: **PASS**
- object authority / placement / companion save: **PASS**
- actual PalmStudio object visual lab A/B: **PASS**
- visual object hash: \`$hash_a\`
- E01-E12: **PASS**
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
else
  echo "失败：无法生成 R4 证据 ZIP。" >&2; exit 1
fi
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$LOCAL_DIR/inventory.tsv" \
  "$EVIDENCE_DIR" "$ZIP_FILE"
echo "R4 acceptance PASS: $EVIDENCE_DIR"
echo "evidence package: $ZIP_FILE"
