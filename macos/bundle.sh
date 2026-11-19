#!/bin/bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
out="$root/dist"
mkdir -p "$out"
stage=$(mktemp -d "$out/.minterm.XXXXXXXX")
app="$stage/Minterm.app"
destination="$out/Minterm.app"
cleanup() {
  if [[ -e "$stage/previous.app" && ! -e "$destination" ]]; then
    mv "$stage/previous.app" "$destination"
  fi
  rm -rf "$stage"
}
trap cleanup EXIT

# Cargo reports the actual executable path for custom target directories too.
cargo build --release --locked --bin minterm --message-format=json-render-diagnostics > "$stage/build.jsonl"
binary=$(
  awk '/"executable"[[:space:]]*:[[:space:]]*"/ { artifact=$0 } END { print artifact }' "$stage/build.jsonl" |
    plutil -extract executable raw -o - -- -
)
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 0755 "$binary" "$app/Contents/MacOS/minterm"
notices="$app/Contents/Resources/Licenses"
mkdir -p "$notices"
cp ui/fonts/*-OFL.txt ui/vendor/*-LICENSE "$notices/"
cp macos/Info.plist "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"

icons="$stage/Minterm.iconset"
mkdir "$icons"
sips -s format png macos/Minterm.svg --out "$stage/icon.png" >/dev/null
for points in 16 32 128 256 512; do
  sips -z "$points" "$points" "$stage/icon.png" --out "$icons/icon_${points}x${points}.png" >/dev/null
  retina=$((points * 2))
  sips -z "$retina" "$retina" "$stage/icon.png" --out "$icons/icon_${points}x${points}@2x.png" >/dev/null
done
iconutil --convert icns --output "$app/Contents/Resources/Minterm.icns" "$icons"
codesign --force --sign - "$app"
codesign --verify --strict "$app"

if [[ -e "$destination" ]]; then
  mv "$destination" "$stage/previous.app"
fi
mv "$app" "$destination"
printf '%s\n' "$destination"
