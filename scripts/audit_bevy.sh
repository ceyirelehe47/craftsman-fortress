#!/usr/bin/env bash
# 依赖版本审计：依赖树中全部 bevy crate 必须为 0.19 系列，Cargo.toml
# 声明必须为 =0.19.x 锁定。CI 与验收入口共用。
set -euo pipefail
cd "$(dirname "$0")/.."

# 白名单：bevy_mikktspace 是独立版本线的官方合作库（由 bevy_render 0.19.1
# 自带依赖，版本号不随 bevy 主版本同步），不属于引擎主版本系列。
BEVY_VERS=$(cargo tree -e normal --format '{p}' 2>/dev/null \
  | grep -E '(^| )bevy(_[a-z_]+)? v' \
  | grep -v 'bevy_mikktspace ' \
  | sed -E 's/.*bevy(_[a-z_]+)? v([0-9]+\.[0-9]+\.[0-9]+).*/\2/' \
  | sort -u)
echo "依赖树中的 bevy 版本："
echo "$BEVY_VERS"
BAD_VERS=$(echo "$BEVY_VERS" | grep -v '^0\.19\.' || true)
if [ -n "$BAD_VERS" ]; then
  echo "审计失败：发现非 0.19 系列 bevy 版本：$BAD_VERS" >&2
  exit 1
fi
# 同时断言 Cargo.toml 声明即为 0.19 系列锁定。
grep -Eq '^bevy = "=0\.19\.[0-9]+"' Cargo.toml \
  || { echo "Cargo.toml bevy 约束非 =0.19.x 锁定" >&2; exit 1; }
echo "Bevy 版本审计通过（0.19 系列锁定）。"
