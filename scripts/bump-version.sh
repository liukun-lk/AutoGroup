#!/bin/bash
set -euo pipefail

if [ "${1-}" = "" ]; then
  echo "用法: $0 <新版本号>"
  echo "示例: $0 0.1.7"
  exit 1
fi

NEW_VERSION="$1"

# 简单校验：x.y.z
if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "错误: 版本号格式不正确，应为 x.y.z (例如: 0.1.7)"
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "正在更新版本号到 $NEW_VERSION..."

# 统一处理 macOS/Linux sed -i 差异
sed_inplace() {
  local expr="$1"
  local file="$2"
  if [[ "${OSTYPE-}" == "darwin"* ]]; then
    sed -i '' "$expr" "$file"
  else
    sed -i "$expr" "$file"
  fi
}

# package.json
PACKAGE_JSON="$ROOT_DIR/package.json"
if [ -f "$PACKAGE_JSON" ]; then
  sed_inplace "s/\"version\": \"[^\"]*\"/\"version\": \"$NEW_VERSION\"/" "$PACKAGE_JSON"
  echo "✓ 已更新 package.json"
else
  echo "✗ 未找到 package.json，跳过"
fi

# src-tauri/Cargo.toml（只替换 [package] 下的 version 行）
CARGO_TOML="$ROOT_DIR/src-tauri/Cargo.toml"
if [ -f "$CARGO_TOML" ]; then
  sed_inplace "s/^version = \"[^\"]*\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
  echo "✓ 已更新 src-tauri/Cargo.toml"
else
  echo "✗ 未找到 src-tauri/Cargo.toml，跳过"
fi

# src-tauri/tauri.conf.json
TAURI_CONF="$ROOT_DIR/src-tauri/tauri.conf.json"
if [ -f "$TAURI_CONF" ]; then
  sed_inplace "s/\"version\": \"[^\"]*\"/\"version\": \"$NEW_VERSION\"/" "$TAURI_CONF"
  echo "✓ 已更新 src-tauri/tauri.conf.json"
else
  echo "✗ 未找到 src-tauri/tauri.conf.json，跳过"
fi

echo "版本号已更新到 $NEW_VERSION"

