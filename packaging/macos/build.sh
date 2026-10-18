#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || { echo 'Build on an Apple Silicon Mac with Xcode 15.3+ and Rust installed.' >&2; exit 1; }
export MACOSX_DEPLOYMENT_TARGET=14.0
cargo build --locked --release --target aarch64-apple-darwin -p omm-cli -p omm-daemon -p omm-helper
swift build --package-path apps/macos -c release --arch arm64
swift_bin=$(swift build --package-path apps/macos -c release --arch arm64 --show-bin-path)
app="$PWD/target/macos/Mihomo.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/bin"
cp "$swift_bin/Mihomo" "$app/Contents/MacOS/Mihomo"
for bundle in "$swift_bin"/*.bundle; do [[ ! -d $bundle ]] || ditto "$bundle" "$app/Contents/Resources/$(basename "$bundle")"; done
for bin in omm omarchy-mihomod omm-network-helper; do cp "target/aarch64-apple-darwin/release/$bin" "$app/Contents/Resources/bin/"; done
cp "$swift_bin/omm-system-proxy" "$app/Contents/Resources/bin/"
archive="$PWD/target/macos/mihomo-darwin-arm64-v1.19.32.gz"
if [[ ! -f $archive ]]; then curl --fail --location --retry 3 --output "$archive" 'https://github.com/MetaCubeX/mihomo/releases/download/v1.19.32/mihomo-darwin-arm64-v1.19.32.gz'; fi
printf '%s  %s\n' '3312a6780652c622890fd4357c6a853bbf865464fd047ac7b7f52dab8de18652' "$archive" | shasum -a 256 -c -
gzip -dc "$archive" > "$app/Contents/Resources/bin/mihomo"
chmod 755 "$app/Contents/Resources/bin/"*
cp packaging/macos/Info.plist "$app/Contents/Info.plist"
# Local Apple Silicon execution uses ad-hoc signing; no Developer ID is needed.
for bin in "$app/Contents/Resources/bin/"*; do codesign --force --sign - "$bin"; done
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
echo "Built: $app"
