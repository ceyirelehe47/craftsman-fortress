#!/usr/bin/env bash
# 初版验收单一入口（任务书 5.1 / 7：一条命令完成全部自动验收，无需用户操作）。
#
# 用法：bash scripts/acceptance.sh [证据目录]
# 默认证据目录 evidence_<timestamp>。脚本依次执行：
#   1. cargo fmt --check
#   2. cargo clippy --all-targets -D warnings
#   3. 依赖版本审计（依赖树中全部 bevy crate 必须为 0.19 系列）
#   4. cargo test --release（单元 + 集成测试）
#   5. cargo build --release
#   6. 运行 --acceptance 模式（脚本相机巡航 + 截图 + 指标 + 自动判定，约 11 分钟）
# 结果写入 <证据目录>/build_checks.json（供应用内 A01 判定读取）与
# <证据目录>/report.json / report.md；任何一步失败立即以非零码退出。

set -euo pipefail
cd "$(dirname "$0")/.."

EVIDENCE_DIR="${1:-evidence_$(date +%Y%m%d_%H%M%S)}"
mkdir -p "$EVIDENCE_DIR"

# Windows (Git Bash) 与 Unix 通用的时间统计。
now_s() { date +%s; }
T_START=$(now_s)

step() { printf '\n===== %s =====\n' "$1"; }

# ---------------------------------------------------------------------------
step "A01-1/6 cargo fmt --check"
cargo fmt --check
FMT_OK=true

# ---------------------------------------------------------------------------
step "A01-2/6 cargo clippy --all-targets -D warnings"
cargo clippy --all-targets -- -D warnings
CLIPPY_OK=true

# ---------------------------------------------------------------------------
step "A01-3/6 依赖版本审计（Bevy 必须仅为 0.19 系列）"
# 提取依赖树中所有 bevy/bevy_* crate 的版本，断言均为 0.19.x。
BEVY_VERS=$(cargo tree -e normal --format '{p}' 2>/dev/null \
  | grep -E '(^| )bevy(_[a-z_]+)? v' \
  | sed -E 's/.*bevy(_[a-z_]+)? v([0-9]+\.[0-9]+\.[0-9]+).*/\2/' \
  | sort -u)
echo "依赖树中的 bevy 版本："
echo "$BEVY_VERS"
BAD_VERS=$(echo "$BEVY_VERS" | grep -v '^0\.19\.' || true)
if [ -n "$BAD_VERS" ]; then
  echo "审计失败：发现非 0.19 系列 bevy 版本：$BAD_VERS" >&2
  exit 1
fi
# 同时断言 Cargo.toml 声明即为 0.19 系列。
grep -Eq '^bevy = "=0\.19\.[0-9]+"' Cargo.toml || { echo "Cargo.toml bevy 约束非 =0.19.x" >&2; exit 1; }

# ---------------------------------------------------------------------------
step "A01-4/6 cargo test --release"
cargo test --release
TESTS_OK=true

# ---------------------------------------------------------------------------
step "A01-5/6 cargo build --release"
cargo build --release
BUILD_OK=true

# ---------------------------------------------------------------------------
step "机器信息"
{
  echo "os: $(uname -s -m -r)"
  echo "rustc: $(rustc --version)"
  echo "cargo: $(cargo --version)"
  echo "date: $(date -Iseconds)"
  echo "logical_cores: ${NUMBER_OF_PROCESSORS:-$(nproc 2>/dev/null || echo '?')}"
} | tee "$EVIDENCE_DIR/machine.txt"

# 写 build_checks.json（应用内 A01 判定读取）。
cat > "$EVIDENCE_DIR/build_checks.json" <<EOF
{
  "fmt": $FMT_OK,
  "clippy": $CLIPPY_OK,
  "tests": $TESTS_OK,
  "release_build": $BUILD_OK
}
EOF

# ---------------------------------------------------------------------------
step "A01-6/6 运行验收巡航（--acceptance，预计约 11 分钟）"
BIN="target/release/craftsman_fortress"
[ -x "$BIN.exe" ] && BIN="$BIN.exe"
"$BIN" --acceptance --evidence "$EVIDENCE_DIR" 2>&1 | tee "$EVIDENCE_DIR/app_stdout.log"
EXIT=${PIPESTATUS[0]}

T_END=$(now_s)
echo ""
echo "===== 验收入口结束（总耗时 $((T_END - T_START))s，应用退出码 $EXIT）====="
echo "证据目录：$EVIDENCE_DIR"
if [ "$EXIT" -ne 0 ]; then
  echo "验收失败：详见 $EVIDENCE_DIR/report.md" >&2
  exit "$EXIT"
fi
echo "A01-A13 运行时判定全部 PASS（A14 需独立 Reviewer 复核）。"
