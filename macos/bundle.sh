#!/bin/bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
out="$root/dist"
mode=${1:-}
if [[ $# -gt 1 || ( $# -eq 1 && $mode != --install && $mode != --dmg ) ]]; then
  echo "Usage: $0 [--install | --dmg]" >&2
  exit 2
fi
if [[ $mode == --install ]]; then
  out="$HOME/Applications"
fi
destination="$out/Minterm.app"
if [[ $mode == --install && ( -e "$destination" || -L "$destination" ) ]]; then
  existing_id=$(plutil -extract CFBundleIdentifier raw -o - "$destination/Contents/Info.plist" 2>/dev/null || true)
  bundle_id=$(plutil -extract CFBundleIdentifier raw -o - macos/Info.plist)
  if [[ "$existing_id" != "$bundle_id" ]]; then
    echo "A different item already occupies $destination. Move it before installing Minterm." >&2
    exit 1
  fi
fi
mkdir -p "$out"
stage=$(mktemp -d "$out/.minterm.XXXXXXXX")
app="$stage/Minterm.app"
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

if [[ $mode == --dmg ]]; then
  if [[ $(lipo -archs "$app/Contents/MacOS/minterm") != arm64 ]]; then
    echo "The distribution DMG requires an Apple Silicon build." >&2
    exit 1
  fi
  image="$stage/image"
  dmg=Minterm-arm64.dmg
  mkdir "$image"
  mv "$app" "$image/"
  ln -s /Applications "$image/Applications"
  hdiutil create -quiet -volname Minterm -srcfolder "$image" -fs HFS+ -format UDZO "$stage/$dmg"
  hdiutil verify -quiet "$stage/$dmg"
  mv -f "$stage/$dmg" "$out/$dmg"
  printf '%s\n' "$out/$dmg"
  exit 0
fi

if [[ -e "$destination" ]]; then
  mv "$destination" "$stage/previous.app"
fi
mv "$app" "$out/"
printf '%s\n' "$destination"
