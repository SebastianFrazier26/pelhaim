#!/usr/bin/env bash
# Builds target/pelhaim.app: the release binary, the icon and an ad-hoc signature.
# Usage: scripts/bundle-macos.sh [--install]   (--install copies the app to ~/Applications)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

install=0
for arg in "$@"; do
  case "$arg" in
    --install) install=1 ;;
    *) echo "usage: $0 [--install]" >&2; exit 2 ;;
  esac
done
[ "$(uname -s)" = Darwin ] || { echo "macOS only" >&2; exit 1; }

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
cargo build --release -p pelhaim

app=target/pelhaim.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/pelhaim "$app/Contents/MacOS/pelhaim"
cp assets/icon/pelhaim.icns "$app/Contents/Resources/pelhaim.icns"

# A launcher pins the mode, so a later change to the CLI's default never changes what the
# Dock icon opens.
cat > "$app/Contents/MacOS/pelhaim-app" <<'SH'
#!/bin/sh
exec "$(dirname "$0")/pelhaim" observe
SH
chmod +x "$app/Contents/MacOS/pelhaim-app"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>pelhaim</string>
  <key>CFBundleExecutable</key><string>pelhaim-app</string>
  <key>CFBundleIconFile</key><string>pelhaim</string>
  <key>CFBundleIdentifier</key><string>dev.pelhaim.app</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>pelhaim</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist" >/dev/null

codesign --force -s - "$app/Contents/MacOS/pelhaim"
codesign --force -s - "$app"
codesign --verify --strict "$app"
echo "built $app"

if [ "$install" = 1 ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/pelhaim.app"
  cp -R "$app" "$HOME/Applications/pelhaim.app"
  echo "installed $HOME/Applications/pelhaim.app"
fi
