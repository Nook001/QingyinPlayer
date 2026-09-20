#!/usr/bin/env bash
# Install into an isolated PREFIX, smoke-start, then uninstall.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
prefix="${PREFIX:-$(mktemp -d "${TMPDIR:-/tmp}/qingyin-prefix.XXXXXX")}"
export PREFIX="$prefix"
export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}"
export QT_QUICK_CONTROLS_STYLE="${QT_QUICK_CONTROLS_STYLE:-Basic}"

bash "$root/packaging/install.sh"
test -x "$prefix/bin/qingyin"
grep -F "Exec=$prefix/bin/qingyin" "$prefix/share/applications/qingyin.desktop" >/dev/null

"$prefix/bin/qingyin" --smoke

bash "$root/packaging/install.sh" uninstall
if [[ -e "$prefix/bin/qingyin" || -e "$prefix/share/applications/qingyin.desktop" ]]; then
  echo "uninstall left files behind" >&2
  exit 1
fi

echo "install smoke passed prefix=$prefix"
