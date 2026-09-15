#!/usr/bin/env bash
# R5 单一验收入口：R4 全回归 + 建筑权威层/三层存档 + 程序化模块建筑视觉双跑。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

EVIDENCE_DIR="${1:-evidence_r5_$(date +%Y%m%d_%H%M%S)}"
MANIFEST="${R3_MANIFEST:-assets/vendor_local/voxel_survival_pack/v1.0/free_sample/selection.tsv}"
WATCHDOG_SEC="${R5_VISUAL_WATCHDOG_SEC:-180}"

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
  echo "失败：R5 证据目录必须被 .gitignore 忽略。" >&2
  exit 1
}
SHA=$(git rev-parse HEAD)
printf '%s\n' "$SHA" > "$EVIDENCE_DIR/commit.txt"
SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT

run_watchdog() {
  label="$1"; log="$2"; shift 2
  "$@" >"$log" 2>&1 &
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
      } > "$EVIDENCE_DIR/r5_watchdog.txt"
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

printf '\n===== R4 full regression and private-cache acquisition =====\n'
R3_ASSET_CACHE_FILE="${R3_ASSET_CACHE_FILE:-}" \
ACCEPTANCE_WATCHDOG_SEC="${ACCEPTANCE_WATCHDOG_SEC:-1500}" \
R2_HEADLESS_WATCHDOG_SEC="${R2_HEADLESS_WATCHDOG_SEC:-300}" \
R3_ASSET_WATCHDOG_SEC="${R3_ASSET_WATCHDOG_SEC:-420}" \
R4_HEADLESS_WATCHDOG_SEC="${R4_HEADLESS_WATCHDOG_SEC:-300}" \
R4_VISUAL_WATCHDOG_SEC="${R4_VISUAL_WATCHDOG_SEC:-180}" \
  bash scripts/r4_acceptance.sh "$EVIDENCE_DIR/r4"
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$EVIDENCE_DIR/r4/report.json" || {
  echo "失败：R4 回归不是 PASS。" >&2
  exit 1
}

# R5 只保留展开后的历史证据。删除子轮 ZIP 与其旧校验清单，避免证据链每轮再嵌套一层；
# 最终由 R5 根级 SHA256SUMS 重新覆盖全部保留文件。不得因此放宽泄漏守卫深度。
find "$EVIDENCE_DIR" -type f -name '*.zip' -delete
find "$EVIDENCE_DIR" -type f -name 'SHA256SUMS.txt' -delete

printf '\n===== R5 headless modular-building layer =====\n'
R5_HEADLESS_WATCHDOG_SEC="${R5_HEADLESS_WATCHDOG_SEC:-300}" \
  bash scripts/r5_headless_checks.sh "$EVIDENCE_DIR/headless"

printf '\n===== R5 procedural building visual lab A/B =====\n'
cargo build --release --example r5_building_lab
LAB="target/release/examples/r5_building_lab"
[ -x "$LAB.exe" ] && LAB="$LAB.exe"
for run in a b; do
  out="$EVIDENCE_DIR/lab_$run"
  code=0
  run_watchdog "r5_building_lab_$run" "$SCRATCH/lab_${run}.log" \
    "$LAB" "$out" || code=$?
  mkdir -p "$out"
  cp "$SCRATCH/lab_${run}.log" "$out/app_stdout.log"
  [ "$code" -eq 0 ] || { cat "$out/app_stdout.log" >&2; exit "$code"; }
  grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$out/report.json" || {
    echo "失败：R5 lab_$run 不是 PASS。" >&2
    exit 1
  }
  hits=$(grep -nE 'panic|ERROR|FATAL|B0004' "$out/app_stdout.log" || true)
  [ -z "$hits" ] || { printf '%s\n' "$hits" >&2; exit 1; }
done
hash_a=$(grep -o '"building_hash":"[^"]*"' "$EVIDENCE_DIR/lab_a/report.json" | head -1 | cut -d'"' -f4)
hash_b=$(grep -o '"building_hash":"[^"]*"' "$EVIDENCE_DIR/lab_b/report.json" | head -1 | cut -d'"' -f4)
[ -n "$hash_a" ] && [ "$hash_a" = "$hash_b" ] || {
  echo "失败：R5 visual 双跑建筑哈希不一致：$hash_a / $hash_b" >&2
  exit 1
}
for name in \
  R5_01_component_catalog.png \
  R5_02_wall_door_window.png \
  R5_03_stair_and_upper_floor.png \
  R5_04_structural_frame.png; do
  sha_a=$(sha256sum "$EVIDENCE_DIR/lab_a/screenshots/$name" | awk '{print $1}')
  sha_b=$(sha256sum "$EVIDENCE_DIR/lab_b/screenshots/$name" | awk '{print $1}')
  [ "$sha_a" = "$sha_b" ] || {
    echo "失败：R5 截图双跑不一致：$name" >&2
    exit 1
  }
done

LOCAL_DIR=$(dirname "$MANIFEST")
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$LOCAL_DIR/inventory.tsv" \
  "$EVIDENCE_DIR"

cat > "$EVIDENCE_DIR/report.json" <<JSON
{
  "commit":"$SHA",
  "overall":"PASS",
  "r4":"PASS",
  "r5_headless":"PASS",
  "r5_visual":"PASS",
  "visual_building_hash":"$hash_a",
  "checks":[
    {"id":"F01","status":"PASS","name":"stable persistent building ids and types"},
    {"id":"F02","status":"PASS","name":"thin canonical slots and quarter turns"},
    {"id":"F03","status":"PASS","name":"structural support and dependent-removal rejection"},
    {"id":"F04","status":"PASS","name":"door window stair semantics"},
    {"id":"F05","status":"PASS","name":"terrain object building cross-layer constraints"},
    {"id":"F06","status":"PASS","name":"place select move rotate cancel delete rollback"},
    {"id":"F07","status":"PASS","name":"building semantic hash order independence"},
    {"id":"F08","status":"PASS","name":"strict CFBLD001 format and triple binding"},
    {"id":"F09","status":"PASS","name":"exact terrain object building roundtrip"},
    {"id":"F10","status":"PASS","name":"corruption fallback and interrupted triple commit"},
    {"id":"F11","status":"PASS","name":"R4 compatibility and frozen earlier formats"},
    {"id":"F12","status":"PASS","name":"procedural visuals screenshots evidence reviewer readiness"}
  ]
}
JSON
cat > "$EVIDENCE_DIR/report.md" <<MD
# R5 modular-building acceptance

- commit: \`$SHA\`
- R4 full regression: **PASS**
- building authority / structural rules / three-layer save: **PASS**
- procedural modular-building visual lab A/B: **PASS**
- visual building hash: \`$hash_a\`
- F01-F12: **PASS**
MD

# r5_headless_checks 生成的子级校验清单在最终打包前移除；根清单覆盖保留的全部文件。
find "$EVIDENCE_DIR" -mindepth 2 -type f -name 'SHA256SUMS.txt' -delete
find "$EVIDENCE_DIR" -type f ! -name SHA256SUMS.txt -print0 \
  | sort -z | xargs -0 sha256sum > "$EVIDENCE_DIR/SHA256SUMS.txt"
ZIP_FILE="${EVIDENCE_DIR}.zip"
rm -f "$ZIP_FILE"
if command -v zip >/dev/null 2>&1; then
  zip -qr "$ZIP_FILE" "$EVIDENCE_DIR"
elif command -v powershell >/dev/null 2>&1; then
  powershell -NoProfile -Command \
    "Compress-Archive -Path '$EVIDENCE_DIR' -DestinationPath '$ZIP_FILE'" >/dev/null
else
  echo "失败：无法生成 R5 证据 ZIP。" >&2
  exit 1
fi
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$LOCAL_DIR/inventory.tsv" \
  "$EVIDENCE_DIR" "$ZIP_FILE"
echo "R5 acceptance PASS: $EVIDENCE_DIR"
echo "evidence package: $ZIP_FILE"
