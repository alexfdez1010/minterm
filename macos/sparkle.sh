#!/bin/bash
set -euo pipefail

# Pinned upstream release, verified before its framework or signing tools run.
version=2.10.1
sha256=1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27
root=$(cd "$(dirname "$0")/.." && pwd)
cache="$root/target/sparkle-$version"
if [[ ! -f "$cache/.verified" || ! -d "$cache/Sparkle.framework" ]]; then
  stage=$(mktemp -d "${TMPDIR:-/tmp}/minterm-sparkle.XXXXXXXX")
  trap 'rm -rf "$stage"' EXIT
  curl -fsSL "https://github.com/sparkle-project/Sparkle/releases/download/$version/Sparkle-$version.tar.xz" -o "$stage/sparkle.tar.xz"
  echo "$sha256  $stage/sparkle.tar.xz" | shasum -a 256 -c >&2
  mkdir "$stage/extracted"
  tar -xJf "$stage/sparkle.tar.xz" -C "$stage/extracted"
  touch "$stage/extracted/.verified"
  mkdir -p "$(dirname "$cache")"
  rm -rf "$cache"
  mv "$stage/extracted" "$cache"
fi
printf '%s\n' "$cache"
