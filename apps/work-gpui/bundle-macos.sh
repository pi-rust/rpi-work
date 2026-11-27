#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: bash %s /path/to/work-gpui /path/to/RPI\ Work.app\n' "$0" >&2
    exit 2
}

[[ $# -eq 2 && $(uname -s) == Darwin ]] || usage
binary=$1
output=$2
[[ -f "$binary" && -x "$binary" && "$output" == *.app ]] || usage
if [[ -e "$output" || -L "$output" ]]; then
    printf 'Destination already exists: %s\n' "$output" >&2
    exit 1
fi

app_dir=$(cd "$(dirname "$0")" && pwd -P)
version=$(awk -F '"' '/^version = / { print $2; exit }' "$app_dir/Cargo.toml")
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    printf 'Expected a numeric application version in Cargo.toml\n' >&2
    exit 1
fi
minimum=$(vtool -show-build "$binary" | awk '$1 == "minos" { print $2; exit }')
if [[ ! "$minimum" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]]; then
    printf 'The executable must be a macOS Mach-O with a deployment target\n' >&2
    exit 1
fi

parent=$(dirname "$output")
mkdir -p "$parent"
stage=$(mktemp -d "$parent/.work-gpui-bundle.XXXXXXXX")
trap 'rm -rf -- "$stage"' EXIT
bundle="$stage/$(basename "$output")"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
install -m 0755 "$binary" "$bundle/Contents/MacOS/work-gpui"
install -m 0644 "$app_dir/../../assets/branding/rpi.icns" "$bundle/Contents/Resources/rpi.icns"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>RPI Work</string>
    <key>CFBundleDisplayName</key><string>RPI Work</string>
    <key>CFBundleExecutable</key><string>work-gpui</string>
    <key>CFBundleIdentifier</key><string>com.pirust.rpiwork</string>
    <key>CFBundleIconFile</key><string>rpi.icns</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>$minimum</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
plutil -lint "$bundle/Contents/Info.plist"
codesign --force --sign - "$bundle"
codesign --verify --strict "$bundle"
mv -n "$bundle" "$parent/"
if [[ -e "$bundle" ]]; then
    printf 'Destination already exists: %s\n' "$output" >&2
    exit 1
fi
printf '%s\n' "$output"
