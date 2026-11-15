#!/bin/sh
# Builds Dozy.app into target/. Pass --universal for an Apple Silicon + Intel binary.
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APP=target/Dozy.app

if [ "${1:-}" = "--universal" ]; then
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin
    BIN=target/dozy-universal
    lipo -create -output "$BIN" target/aarch64-apple-darwin/release/dozy target/x86_64-apple-darwin/release/dozy
else
    cargo build --release
    BIN=target/release/dozy
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Dozy"
cp assets/Dozy.icns "$APP/Contents/Resources/Dozy.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Dozy</string>
    <key>CFBundleDisplayName</key><string>Dozy</string>
    <key>CFBundleIdentifier</key><string>app.dozy</string>
    <key>CFBundleExecutable</key><string>Dozy</string>
    <key>CFBundleIconFile</key><string>Dozy</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --sign - "$APP"
echo "Built $APP"
