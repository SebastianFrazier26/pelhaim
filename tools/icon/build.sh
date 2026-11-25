#!/usr/bin/env bash
# Regenerates the icon PNGs and iconset from source, then (on macOS) assets/icon/pelhaim.icns.
set -euo pipefail
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"
cargo run -q -p pelhaim-board --example icon
if command -v iconutil >/dev/null; then
  iconutil -c icns assets/icon/pelhaim.iconset -o assets/icon/pelhaim.icns
  echo "wrote assets/icon/pelhaim.icns"
else
  echo "iconutil not found (macOS only): pelhaim.icns left as is" >&2
fi
