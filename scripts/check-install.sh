#!/usr/bin/env bash
# Install into an isolated PREFIX, smoke-start, then uninstall.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
prefix="${PREFIX:-$(mktemp -d "${TMPDIR:-/tmp}/qingyin-prefix.XXXXXX")}"
app_id="io.github.nook001.qingyin"
export PREFIX="$prefix"
export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}"
export QT_QUICK_CONTROLS_STYLE="${QT_QUICK_CONTROLS_STYLE:-Basic}"

bash "$root/packaging/install.sh"
test -x "$prefix/bin/qingyin"
test -f "$prefix/share/applications/$app_id.desktop"
test -f "$prefix/share/icons/hicolor/scalable/apps/$app_id.svg"
test -f "$prefix/share/metainfo/$app_id.metainfo.xml"
test ! -e "$prefix/share/applications/qingyin.desktop"
grep -F "Exec=qingyin" "$prefix/share/applications/$app_id.desktop" >/dev/null
grep -F "Icon=$app_id" "$prefix/share/applications/$app_id.desktop" >/dev/null
grep -F "<id>$app_id</id>" "$prefix/share/metainfo/$app_id.metainfo.xml" >/dev/null
if grep -R -n -E '/home/|/mnt/workspace' \
  "$prefix/share/applications" \
  "$prefix/share/metainfo" \
  "$prefix/share/icons" >/dev/null; then
  echo "install tree contains a local absolute path" >&2
  exit 1
fi

"$prefix/bin/qingyin" --smoke

bash "$root/packaging/install.sh" uninstall
if [[ -e "$prefix/bin/qingyin" \
  || -e "$prefix/share/applications/$app_id.desktop" \
  || -e "$prefix/share/icons/hicolor/scalable/apps/$app_id.svg" \
  || -e "$prefix/share/metainfo/$app_id.metainfo.xml" \
  || -e "$prefix/share/applications/qingyin.desktop" ]]; then
  echo "uninstall left files behind" >&2
  exit 1
fi

echo "install smoke passed prefix=$prefix"
