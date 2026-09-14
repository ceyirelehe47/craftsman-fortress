#!/usr/bin/env bash
# R3.1 第三方素材泄漏守卫：路径、扩展名、逐文件内容哈希与嵌套 ZIP 全部检查。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

MANIFEST=''
INVENTORY=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    --manifest) MANIFEST="${2:?--manifest requires a path}"; shift 2 ;;
    --inventory) INVENTORY="${2:?--inventory requires a path}"; shift 2 ;;
    --) shift; break ;;
    -*) echo "未知选项：$1" >&2; exit 2 ;;
    *) break ;;
  esac
done
TARGETS=("$@")

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

forbidden=$(printf '%s\n' "$tracked" | grep -Ei '(^|/)(Free_Sample|Survival_Pack_v1\.0)\.rar$|\.(glb|gltf|fbx|obj|rar)$' || true)
if [ -n "$forbidden" ]; then
  echo "失败：仓库中存在禁止跟踪的素材/压缩包：" >&2
  printf '%s\n' "$forbidden" >&2
  exit 1
fi

TMP_ROOT=$(mktemp -d)
HASH_FILE="$TMP_ROOT/forbidden_hashes.txt"
: > "$HASH_FILE"
cleanup() { rm -rf "$TMP_ROOT"; }
trap cleanup EXIT

meta() {
  key="$1"; file="$2"
  sed -n "s/^# ${key}=//p; s/^#${key}=//p" "$file" | head -1 | tr -d '\r'
}
if [ -n "$MANIFEST" ] || [ -n "$INVENTORY" ]; then
  [ -s "$MANIFEST" ] && [ -s "$INVENTORY" ] || {
    echo "失败：--manifest 与 --inventory 必须同时提供。" >&2
    exit 2
  }
  archive_sha=$(meta archive_sha256 "$MANIFEST")
  [ ${#archive_sha} -eq 64 ] || { echo "失败：manifest archive_sha256 无效。" >&2; exit 1; }
  printf '%s\n' "$archive_sha" >> "$HASH_FILE"
  awk -F '\t' 'NR>1 && length($1)==64 {print $1}' "$INVENTORY" >> "$HASH_FILE"
  sort -u "$HASH_FILE" -o "$HASH_FILE"
fi

path_forbidden() {
  path="$1"
  printf '%s' "$path" | grep -Eiq '(^|/)(third_party_raw|assets/vendor_local|vendor_local/voxel_survival_pack)(/|$)|(^|/)(Free_Sample|Survival_Pack_v1\.0)\.rar$|\.(glb|gltf|fbx|obj|rar)$'
}

hash_forbidden() {
  file="$1"
  [ -s "$HASH_FILE" ] || return 1
  sha=$(sha256sum "$file" | awk '{print $1}')
  grep -Fxq "$sha" "$HASH_FILE"
}

extract_zip() {
  zip_file="$1"; dest="$2"
  mkdir -p "$dest"
  if command -v unzip >/dev/null 2>&1; then
    # PowerShell Compress-Archive 生成的 ZIP 本地文件头使用反斜杠，
    # Info-ZIP unzip 能完整解包但以退出码 1（警告）结束；>1 才是解包失败。
    rc=0
    unzip -qq "$zip_file" -d "$dest" || rc=$?
    [ "$rc" -le 1 ] || {
      echo "失败：unzip 无法展开 ZIP：$zip_file（exit=$rc）" >&2
      return 1
    }
  elif command -v bsdtar >/dev/null 2>&1; then
    bsdtar -xf "$zip_file" -C "$dest"
  elif [ -x /c/Windows/System32/tar.exe ]; then
    /c/Windows/System32/tar.exe -xf "$zip_file" -C "$dest"
  elif command -v powershell >/dev/null 2>&1; then
    powershell -NoProfile -Command \
      "Expand-Archive -LiteralPath '$zip_file' -DestinationPath '$dest' -Force" >/dev/null
  else
    echo "失败：无法展开 ZIP（缺少 unzip/bsdtar/Windows tar/PowerShell）：$zip_file" >&2
    return 1
  fi
}

scan_dir() {
  root="$1"; depth="$2"
  while IFS= read -r -d '' file; do
    rel=${file#"$root"/}
    if path_forbidden "$rel"; then
      echo "失败：证据/发布内容包含禁止路径或格式：$file" >&2
      return 1
    fi
    if hash_forbidden "$file"; then
      echo "失败：证据/发布内容与第三方模型/贴图/原包字节完全一致：$file" >&2
      return 1
    fi
    case "${file,,}" in
      *.zip)
        [ "$depth" -lt 3 ] || { echo "失败：嵌套 ZIP 深度超过 3：$file" >&2; return 1; }
        nested="$TMP_ROOT/nested_${RANDOM}_${depth}"
        extract_zip "$file" "$nested"
        scan_dir "$nested" $((depth + 1))
        ;;
    esac
  done < <(find "$root" -type f -print0)
}

# 已跟踪文件也要做内容哈希检查，防止贴图改名后混入仓库。
if [ -s "$HASH_FILE" ]; then
  while IFS= read -r file; do
    [ -f "$file" ] || continue
    if hash_forbidden "$file"; then
      echo "失败：Git 跟踪文件与第三方素材字节一致：$file" >&2
      exit 1
    fi
  done <<< "$tracked"
fi

for target in "${TARGETS[@]}"; do
  [ -e "$target" ] || continue
  if [ -d "$target" ]; then
    scan_dir "$target" 0
  else
    if path_forbidden "$(basename "$target")"; then
      echo "失败：禁止把素材文件作为证据/发布资产：$target" >&2
      exit 1
    fi
    if hash_forbidden "$target"; then
      echo "失败：发布资产本身与第三方素材/原包字节一致：$target" >&2
      exit 1
    fi
    case "${target,,}" in
      *.zip)
        unpacked="$TMP_ROOT/top_${RANDOM}"
        extract_zip "$target" "$unpacked"
        scan_dir "$unpacked" 0
        ;;
    esac
  fi
done

echo "第三方素材授权边界检查通过：路径、格式、内容哈希和嵌套 ZIP 均无泄漏。"
