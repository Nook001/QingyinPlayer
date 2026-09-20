#!/usr/bin/env bash
# Install Qingyin into a prefix. QML is embedded in the executable.
# Runtime needs Qt 6 Quick Controls and GStreamer (see message after install).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
prefix="${PREFIX:-/usr/local}"
bindir="$prefix/bin"
applicationsdir="$prefix/share/applications"
binary="$bindir/qingyin"
desktop="$applicationsdir/qingyin.desktop"

uninstall() {
  rm -f "$binary" "$desktop"
  cat <<EOF
Removed:
  $binary
  $desktop
EOF
}

if [[ "${1:-}" == uninstall ]]; then
  uninstall
  exit 0
fi

cd "$root"
cargo build --release -p qingyin-ui-bridge --bin qingyin

install -d "$bindir" "$applicationsdir"
install -m 0755 "$root/target/release/qingyin" "$binary"
tmp_desktop="$(mktemp)"
sed "s|^Exec=qingyin$|Exec=$binary|" "$root/packaging/qingyin.desktop" >"$tmp_desktop"
install -m 0644 "$tmp_desktop" "$desktop"
rm -f "$tmp_desktop"

cat <<EOF
Installed:
  $binary
  $desktop

Resources:
  QML pages are embedded at qrc:/qml inside $binary. No extra qml/ tree is required.

Runtime packages (Debian/Ubuntu names):
  qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts
  qml6-module-qtquick-dialogs qml6-module-qtquick-window
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-pipewire

Verify:
  QT_QPA_PLATFORM=offscreen $binary --smoke

Uninstall:
  PREFIX=$prefix bash $root/packaging/install.sh uninstall
EOF
