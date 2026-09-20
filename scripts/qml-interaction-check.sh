#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
runner="${QMLTESTRUNNER:-}"
if [[ -z "$runner" ]]; then
  for candidate in /usr/lib/qt6/bin/qmltestrunner qmltestrunner6 qmltestrunner; do
    if command -v "$candidate" >/dev/null 2>&1 || [[ -x "$candidate" ]]; then
      runner="$candidate"
      break
    fi
  done
fi
if [[ -z "$runner" ]]; then
  echo "qmltestrunner not found; skip offscreen QML interaction checks" >&2
  exit 0
fi

export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}"
export QT_QUICK_CONTROLS_STYLE="${QT_QUICK_CONTROLS_STYLE:-Basic}"
"$runner" -input "$root/qml/tst_interactions.qml" -import "$root/qml"
