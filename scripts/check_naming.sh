#!/usr/bin/env bash
# 旧名称禁用检查：当前语境不得出现历史占位命名（决策记录 D-17）。
# 命中白名单文件的内容视为历史记录放行；其余任何命中都以非零码失败。
set -euo pipefail
cd "$(dirname "$0")/.."

# 显式白名单：允许出现禁用词的文件。
# - docs/DECISIONS.md：历史决策记录（原文可考）；
# - 本脚本自身：禁用词定义行是功能性构造，不属当前语境使用。
WHITELIST=(
  "docs/DECISIONS.md"
  "scripts/check_naming.sh"
)

# 禁用词：正式名称统一为"工匠要塞"；历史占位命名与需求文档版本引用
# 不得出现在当前语境。
FORBIDDEN='机械纪元|暂名|任务书|V0\.1|V0\.2'

fail=0
while IFS= read -r f; do
  skip=false
  for w in "${WHITELIST[@]}"; do
    if [ "$f" = "$w" ]; then skip=true; break; fi
  done
  if $skip; then continue; fi
  hits=$(grep -nE "$FORBIDDEN" -- "$f" 2>/dev/null || true)
  if [ -n "$hits" ]; then
    echo "禁用旧名称命中: $f"
    printf '%s\n' "$hits" | sed 's/^/    /'
    fail=1
  fi
done < <(git ls-files)

if [ "$fail" -ne 0 ]; then
  echo "旧名称检查失败：当前语境不得使用历史占位命名（白名单见脚本头部）。" >&2
  exit 1
fi
echo "旧名称检查通过（白名单 ${#WHITELIST[@]} 个文件）。"
