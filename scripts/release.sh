#!/usr/bin/env bash
set -euo pipefail

cargo_toml="Cargo.toml"

if [[ ! -f "$cargo_toml" ]]; then
  echo "error: Cargo.toml not found" >&2
  exit 1
fi

current_version=$(awk '
  /^\[package\]$/ { in_package = 1; next }
  /^\[/ { in_package = 0 }
  in_package && /^version[[:space:]]*=/ {
    gsub(/^[^=]*=[[:space:]]*"|"[[:space:]]*$/, "")
    print
    exit
  }
' "$cargo_toml")

if [[ -z "$current_version" ]]; then
  echo "error: package version not found in Cargo.toml" >&2
  exit 1
fi

echo "Current version: $current_version"
read -r -p "New version: " new_version

if [[ -z "$new_version" ]]; then
  echo "error: new version is required" >&2
  exit 1
fi

gh workflow run release.yml --ref main -f "version=$new_version"
