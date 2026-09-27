#!/usr/bin/env bash
# Install Qingyin into ${DESTDIR}${PREFIX}. QML is embedded in the executable.
# Runtime needs Qt 6 Quick Controls and GStreamer (see the message after install).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
prefix="${PREFIX:-/usr/local}"
destdir="${DESTDIR:-}"
app_id="io.github.nook001.qingyin"
payload="$root/packaging/arch"
rootdir="${destdir}${prefix}"

bindir="$rootdir/bin"
applicationsdir="$rootdir/share/applications"
iconsdir="$rootdir/share/icons/hicolor/scalable/apps"
metainfodir="$rootdir/share/metainfo"
binary="$bindir/qingyin"
desktop="$applicationsdir/$app_id.desktop"
icon="$iconsdir/$app_id.svg"
metainfo="$metainfodir/$app_id.metainfo.xml"

uninstall() {
  rm -f "$binary" "$desktop" "$icon" "$metainfo" "$applicationsdir/qingyin.desktop"
  cat <<EOF
Removed:
  $binary
  $desktop
  $icon
  $metainfo
EOF
}

if [[ "${1:-}" == uninstall ]]; then
  uninstall
  exit 0
fi

if [[ "${SKIP_BUILD:-}" != 1 ]]; then
  cd "$root"
  # Panic locations otherwise keep the builder's cargo and source paths.
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$root=/usr/src/qingyin --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/usr/src/cargo"
  cargo build --release --locked -p qingyin-ui-bridge --bin qingyin
fi

install -d "$bindir" "$applicationsdir" "$iconsdir" "$metainfodir"
install -m 0755 "$root/target/release/qingyin" "$binary"
install -m 0644 "$payload/$app_id.desktop" "$desktop"
install -m 0644 "$payload/$app_id.svg" "$icon"
install -m 0644 "$payload/$app_id.metainfo.xml" "$metainfo"

cat <<EOF
Installed:
  $binary
  $desktop
  $icon
  $metainfo

Resources:
  QML pages are embedded at qrc:/qml inside $binary. No extra qml/ tree is required.

Runtime packages (Debian/Ubuntu names):
  qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts
  qml6-module-qtquick-dialogs qml6-module-qtquick-window qml6-module-qtquick-effects
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-pipewire

Verify:
  QT_QPA_PLATFORM=offscreen $binary --smoke

Uninstall:
  PREFIX=$prefix DESTDIR=${destdir:-} bash $root/packaging/install.sh uninstall
EOF
