#!/usr/bin/env bash
# 初版验收单一入口：一条命令完成全部自动验收，无需用户操作。
#
# 用法：bash scripts/acceptance.sh [证据目录]
# 默认证据目录 evidence_<timestamp>。流程：
#   0. 前置守卫：工作区/暂存区干净（未提交改动即失败）并记录完整提交 SHA；
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
#   9. 证据完整性检查（report.json 必须存在）
#  10. 日志错误扫描（panic / ERROR / FATAL / 持续重复错误）
#  11. 生成全证据 SHA-256 清单（SHA256SUMS.txt）并打包 ZIP
# 结果写入 <证据目录>/build_checks.json（供应用内 A01 判定读取）与
# <证据目录>/report.json / report.md；任何一步失败都以非零码退出，
# 失败路径同样尽量留下日志、退出状态与部分校验清单。

set -euo pipefail
cd "$(dirname "$0")/.."

EVIDENCE_DIR="${1:-evidence_$(date +%Y%m%d_%H%M%S)}"
# 应用阶段总超时（秒）：默认 1200s；前置编译时间不计入，可用环境变量覆盖。
WATCHDOG_SEC="${ACCEPTANCE_WATCHDOG_SEC:-1200}"

now_s() { date +%s; }
T_START=$(now_s)

step() { printf '\n===== %s =====\n' "$1"; }

# 失败/退出时的收尾：尽量留下部分校验清单（成功路径的完整清单在末尾生成）。
on_exit() {
  local code=$?
  if [ "$code" -ne 0 ] && [ -d "$EVIDENCE_DIR" ]; then
    printf '%s\n' "$code" > "$EVIDENCE_DIR/last_exit_code.txt" 2>/dev/null || true
    find "$EVIDENCE_DIR" -type f -print0 2>/dev/null \
      | xargs -0 sha256sum > "$EVIDENCE_DIR/SHA256SUMS.partial.txt" 2>/dev/null || true
  fi
}
trap on_exit EXIT

# ---------------------------------------------------------------------------
step "前置守卫 1/2：工作区与暂存区必须干净"
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
CPU_NAME=$(powershell -NoProfile -Command "(Get-CimInstance Win32_Processor).Name" 2>/dev/null \
  | tr -d '\r' | head -1)
[ -z "$CPU_NAME" ] && CPU_NAME=$(wmic cpu get name 2>/dev/null | tr -d '\r' | sed -n 2p)
[ -z "$CPU_NAME" ] && CPU_NAME="$(uname -p)"
{
  echo "os: $(uname -s -m -r)"
  echo "cpu: $CPU_NAME"
  echo "rustc: $(rustc --version)"
  echo "cargo: $(cargo --version)"
  echo "date: $(date -Iseconds)"
  echo "logical_cores: ${NUMBER_OF_PROCESSORS:-$(nproc 2>/dev/null || echo '?')}"
  echo "git_head: $GIT_SHA"
  echo "evidence_dir: $EVIDENCE_DIR"
} | tee "$EVIDENCE_DIR/machine.txt"
# 完整命令行由应用写入 machine.json（command_line 字段）。

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
step "证据完整性检查"
for f in report.json report.md metrics.csv; do
  if [ ! -s "$EVIDENCE_DIR/$f" ]; then
    echo "失败：证据缺失 $EVIDENCE_DIR/$f（应用异常终止或未完成判定）。" >&2
    exit 1
  fi
done
echo "report.json / report.md / metrics.csv 齐备。"

# ---------------------------------------------------------------------------
step "日志错误扫描（panic / ERROR / FATAL / 持续重复）"
LOG="$EVIDENCE_DIR/app_stdout.log"
# 1) panic / FATAL / ERROR：0 容忍（覆盖 "panicked at ..." 与 Bevy 的
#    "Encountered a panic in system ..." 两种形态，日志级别字段与正文均算）。
SCAN_HITS=$(grep -nE "panic|FATAL|ERROR" "$LOG" || true)
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
if [ "$EXIT" -ne 0 ]; then
  T_END=$(now_s)
  echo ""
  echo "===== 验收入口结束（总耗时 $((T_END - T_START))s，应用退出码 $EXIT）=====" >&2
  echo "验收失败：详见 $EVIDENCE_DIR/report.md" >&2
  exit "$EXIT"
fi

# ---------------------------------------------------------------------------
step "证据校验与打包"
# 全部证据文件的 SHA-256 清单（清单自身不含自身）。
find "$EVIDENCE_DIR" -type f ! -name "SHA256SUMS.txt" -print0 \
  | xargs -0 sha256sum > "$EVIDENCE_DIR/SHA256SUMS.txt"
rm -f "$EVIDENCE_DIR/SHA256SUMS.partial.txt" "$EVIDENCE_DIR/last_exit_code.txt"
# ZIP 打包：优先 zip，其次 Windows 自带 bsdtar（-a 按后缀产出 zip），
# 最后 PowerShell Compress-Archive。zip 生成在仓库根目录下。
ZIP_FILE="${EVIDENCE_DIR}.zip"
rm -f "$ZIP_FILE"
BSDTAR="/c/Windows/System32/tar.exe"
REPO_NAME=$(basename "$(pwd)")
if command -v zip > /dev/null 2>&1; then
  zip -qr "$ZIP_FILE" "$EVIDENCE_DIR"
elif [ -x "$BSDTAR" ]; then
  (cd .. && "$BSDTAR" -a -cf "$REPO_NAME/$ZIP_FILE" "$REPO_NAME/$EVIDENCE_DIR")
elif command -v powershell > /dev/null 2>&1; then
  powershell -NoProfile -Command "Compress-Archive -Path '$EVIDENCE_DIR' -DestinationPath '$ZIP_FILE'" \
    > /dev/null 2>&1
fi
if [ -f "$ZIP_FILE" ]; then
  sha256sum "$ZIP_FILE" >> "$EVIDENCE_DIR/SHA256SUMS.txt"
  echo "证据包：$ZIP_FILE"
else
  echo "警告：ZIP 打包不可用（zip/tar/PowerShell 均失败），仅保留证据目录。" >&2
fi

T_END=$(now_s)
echo ""
echo "===== 验收入口结束（总耗时 $((T_END - T_START))s，应用退出码 $EXIT）====="
echo "证据目录：$EVIDENCE_DIR"
echo "A01-A13 运行时判定全部 PASS（A14 需独立 Reviewer 复核）。"
