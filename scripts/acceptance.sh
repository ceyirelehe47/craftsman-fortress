#!/usr/bin/env bash
# 初版验收单一入口：一条命令完成全部自动验收，无需用户操作。
#
# 用法：bash scripts/acceptance.sh [证据目录]
# 默认证据目录 evidence_<timestamp>。流程：
#   0. 前置守卫：工作区干净（未提交改动即失败）并记录完整提交 SHA；
#      证据目录必须全新（已存在非空目录立即失败，禁止复用旧截图）
#   1. cargo fmt --check
#   2. cargo clippy --all-targets -D warnings
#   3. 依赖版本审计（依赖树中全部 bevy crate 必须为 0.19 系列）
#   4. 旧名称扫描（正式命名同步检查，见 scripts/check_naming.sh）
#   5. cargo test --release（单元 + 集成测试）
#   6. cargo build --release
#   7. 机器信息（machine.txt + commit.txt）
#   8. 运行 --acceptance 模式（脚本相机巡航 + 截图 + 指标 + 自动判定，
#      约 11 分钟；带总超时 watchdog，卡死自动失败并留下报告）
#   9. 日志错误扫描（panic / ERROR / FATAL / 持续重复错误）
# 结果写入 <证据目录>/build_checks.json（供应用内 A01 判定读取）与
# <证据目录>/report.json / report.md；任何一步失败都以非零码退出。

set -euo pipefail
cd "$(dirname "$0")/.."

EVIDENCE_DIR="${1:-evidence_$(date +%Y%m%d_%H%M%S)}"

now_s() { date +%s; }
T_START=$(now_s)

step() { printf '\n===== %s =====\n' "$1"; }

# ---------------------------------------------------------------------------
step "前置守卫 1/2：工作区必须干净"
if [ -n "$(git status --porcelain)" ]; then
  echo "失败：工作区存在未提交改动，验收必须针对干净工作区运行。" >&2
  git status --porcelain >&2
  exit 1
fi
GIT_SHA=$(git rev-parse HEAD)
echo "验收提交：$GIT_SHA"

step "前置守卫 2/2：证据目录必须全新"
if [ -e "$EVIDENCE_DIR" ] && [ -n "$(ls -A "$EVIDENCE_DIR" 2>/dev/null)" ]; then
  echo "失败：证据目录 $EVIDENCE_DIR 已存在且非空（禁止复用旧证据/旧截图）。" >&2
  echo "请更换证据目录名后重试。" >&2
  exit 1
fi
mkdir -p "$EVIDENCE_DIR"
printf '%s\n' "$GIT_SHA" > "$EVIDENCE_DIR/commit.txt"

# Windows (Git Bash) 与 Unix 通用的时间统计。

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
bash scripts/audit_bevy.sh

# ---------------------------------------------------------------------------
step "A01-3b 旧名称扫描"
bash scripts/check_naming.sh

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
  echo "git_head: $GIT_SHA"
  echo "evidence_dir: $EVIDENCE_DIR"
} | tee "$EVIDENCE_DIR/machine.txt"
# GPU/驱动/图形后端/窗口模式/完整命令行由应用在 Ready 时写入 machine.json。

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
step "A01-6/6 运行验收巡航（--acceptance，预计约 11 分钟，带 watchdog）"
BIN="target/release/craftsman_fortress"
[ -x "$BIN.exe" ] && BIN="$BIN.exe"
# 应用侧时间线 628s + 启动窗口/收尾 GIF 组装余量 => 总超时 900s。
WATCHDOG_SEC=900
"$BIN" --acceptance --evidence "$EVIDENCE_DIR" \
  > "$EVIDENCE_DIR/app_stdout.log" 2>&1 &
APP_PID=$!
DEADLINE=$(( $(now_s) + WATCHDOG_SEC ))
app_exited=0
heartbeat=0
while :; do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    app_exited=1
    break
  fi
  if [ "$(now_s)" -ge "$DEADLINE" ]; then
    break
  fi
  sleep 5
  heartbeat=$((heartbeat + 1))
  if [ $((heartbeat % 12)) -eq 0 ]; then
    echo "…应用运行中（已等待 $((heartbeat * 5))s / 上限 ${WATCHDOG_SEC}s）"
  fi
done
if [ "$app_exited" -ne 1 ]; then
  {
    echo "watchdog：应用超过 ${WATCHDOG_SEC}s 未退出，已强制终止（验收失败）"
    echo "提交：$GIT_SHA"
    echo "时间：$(date -Iseconds)"
    echo ""
    echo "===== app_stdout.log 尾部 60 行 ====="
    tail -n 60 "$EVIDENCE_DIR/app_stdout.log"
  } > "$EVIDENCE_DIR/watchdog.txt"
  cat "$EVIDENCE_DIR/watchdog.txt" >&2
  exit 124
fi
EXIT=0
wait "$APP_PID" || EXIT=$?
# 控制台回显日志尾部（全量在证据目录）。
tail -n 30 "$EVIDENCE_DIR/app_stdout.log"

# ---------------------------------------------------------------------------
step "日志错误扫描（panic / ERROR / FATAL / 持续重复）"
LOG="$EVIDENCE_DIR/app_stdout.log"
# 1) panic / FATAL / ERROR：0 容忍（日志级别字段与正文中出现均算命中）。
SCAN_HITS=$(grep -nE "panicked|FATAL|ERROR" "$LOG" || true)
if [ -n "$SCAN_HITS" ]; then
  echo "日志扫描失败：发现错误信号：" >&2
  printf '%s\n' "$SCAN_HITS" | head -20 >&2
  exit 1
fi
# 2) 持续重复错误/告警：同一行（去时间戳前缀）重复 ≥50 次即失败。
DUP_HITS=$(sed -E 's/^[0-9T:.+-]+(Z)? //' "$LOG" | sort | uniq -c | awk '$1 >= 50 {print}' || true)
if [ -n "$DUP_HITS" ]; then
  echo "日志扫描失败：发现持续重复输出（≥50 次）：" >&2
  printf '%s\n' "$DUP_HITS" | head -10 >&2
  exit 1
fi
echo "日志扫描通过（无 panic/ERROR/FATAL，无持续重复输出）。"

# ---------------------------------------------------------------------------
T_END=$(now_s)
echo ""
echo "===== 验收入口结束（总耗时 $((T_END - T_START))s，应用退出码 $EXIT）====="
echo "证据目录：$EVIDENCE_DIR"
if [ "$EXIT" -ne 0 ]; then
  echo "验收失败：详见 $EVIDENCE_DIR/report.md" >&2
  exit "$EXIT"
fi
echo "A01-A13 运行时判定全部 PASS（A14 需独立 Reviewer 复核）。"
