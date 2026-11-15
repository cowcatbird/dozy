#!/bin/sh
# Builds target/Dozy.app, installs it to /Applications and opens it.
#   --universal   Apple Silicon + Intel binary
#   --no-install  only build target/Dozy.app
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APP=target/Dozy.app
UNIVERSAL=0
INSTALL=1
for arg in "$@"; do
    case "$arg" in
        --universal) UNIVERSAL=1 ;;
        --no-install) INSTALL=0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

if [ "$UNIVERSAL" = 1 ]; then
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

if [ "$INSTALL" = 1 ]; then
    # Quit this user's running copy (it restores the display on the way out), then swap in the new one.
    running() { pgrep -x -U "$(id -u)" Dozy >/dev/null; }
    wait_quit() { i=0; while running && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done; ! running; }
    if running; then
        osascript -e 'quit app id "app.dozy"' >/dev/null 2>&1 || true
        wait_quit || { pkill -TERM -x -U "$(id -u)" Dozy; wait_quit; } || {
            echo "Dozy didn't quit; quit it from the menu bar and run this again." >&2
            exit 1
        }
    fi
    rm -rf /Applications/Dozy.app
    ditto "$APP" /Applications/Dozy.app
    open /Applications/Dozy.app
    echo "Installed /Applications/Dozy.app"
fi
