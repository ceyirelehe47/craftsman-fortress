#!/usr/bin/env bash
# R3 授权边界守卫：PalmStudio 原始/处理后模型只允许存在于本地忽略目录，
# 不能进入 Git 索引、任务包、证据 ZIP 或 GitHub Release。
set -euo pipefail
cd "$(dirname "$0")/.."

for required in 'third_party_raw/' 'assets/vendor_local/'; do
  grep -Fxq "$required" .gitignore || {
    echo "失败：.gitignore 缺少本地素材目录 $required" >&2
    exit 1
  }
done

tracked=$(git ls-files)
for prefix in 'third_party_raw/' 'assets/vendor_local/'; do
  if printf '%s\n' "$tracked" | grep -Fq "$prefix"; then
    echo "失败：检测到被 Git 跟踪的第三方素材路径：$prefix" >&2
    printf '%s\n' "$tracked" | grep -F "$prefix" >&2 || true
    exit 1
  fi
done

# 额外防线：不允许已知下载包名或常见模型格式进入仓库。
forbidden=$(printf '%s\n' "$tracked" | grep -Ei '(^|/)(Free_Sample|Survival_Pack_v1\.0)\.rar$|\.(glb|gltf|fbx|obj|rar)$' || true)
if [ -n "$forbidden" ]; then
  echo "失败：仓库中存在禁止跟踪的素材/压缩包：" >&2
  printf '%s\n' "$forbidden" >&2
  exit 1
fi

# 可选：扫描证据目录或发布暂存目录，确保没有把模型文件打进证据包。
if [ "$#" -gt 0 ]; then
  for target in "$@"; do
    [ -e "$target" ] || continue
    if [ -d "$target" ]; then
      leaked=$(find "$target" -type f \( \
        -iname '*.glb' -o -iname '*.gltf' -o -iname '*.fbx' -o -iname '*.obj' -o \
        -iname '*.rar' -o -iname 'Free_Sample.rar' -o -iname 'Survival_Pack_v1.0.rar' \
      \) -print || true)
      if [ -n "$leaked" ]; then
        echo "失败：证据/发布目录包含禁止再分发的素材：$target" >&2
        printf '%s\n' "$leaked" >&2
        exit 1
      fi
    else
      case "${target##*.}" in
        glb|gltf|fbx|obj|rar)
          echo "失败：禁止把素材文件作为证据/发布资产：$target" >&2
          exit 1
          ;;
        zip)
          if command -v unzip >/dev/null 2>&1; then
            leaked=$(unzip -Z1 "$target" | grep -Ei '\.(glb|gltf|fbx|obj|rar)$|(^|/)(Free_Sample|Survival_Pack_v1\.0)\.rar$' || true)
          elif command -v bsdtar >/dev/null 2>&1; then
            leaked=$(bsdtar -tf "$target" | tr -d '\r' | grep -Ei '\.(glb|gltf|fbx|obj|rar)$|(^|/)(Free_Sample|Survival_Pack_v1\.0)\.rar$' || true)
          elif [ -x /c/Windows/System32/tar.exe ]; then
            # Windows 自带 bsdtar 以 tar 名义提供；列表输出为 CRLF 行尾，需去掉。
            leaked=$(/c/Windows/System32/tar.exe -tf "$target" | tr -d '\r' | grep -Ei '\.(glb|gltf|fbx|obj|rar)$|(^|/)(Free_Sample|Survival_Pack_v1\.0)\.rar$' || true)
          else
            echo "失败：无法检查 ZIP 内容（缺少 unzip/bsdtar）：$target" >&2
            exit 1
          fi
          if [ -n "$leaked" ]; then
            echo "失败：ZIP 内包含禁止再分发的素材：$target" >&2
            printf '%s\n' "$leaked" >&2
            exit 1
          fi
          ;;
      esac
    fi
  done
fi

echo "第三方素材授权边界检查通过：仓库与指定证据目录均未包含模型原件。"
