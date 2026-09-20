#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
qmllint_bin="${QMLLINT:-}"
if [[ -z "$qmllint_bin" ]]; then
  for candidate in /usr/lib/qt6/bin/qmllint /usr/bin/qmllint6 qmllint; do
    if command -v "$candidate" >/dev/null 2>&1 || [[ -x "$candidate" ]]; then
      qmllint_bin="$candidate"
      break
    fi
  done
fi
if [[ -z "$qmllint_bin" ]]; then
  echo "qmllint not found" >&2
  exit 1
fi

shopt -s nullglob
files=()
for file in "$root"/qml/*.qml; do
  base="$(basename "$file")"
  if [[ "$base" == tst_* ]]; then
    continue
  fi
  files+=("$file")
done
output="$("$qmllint_bin" -I "$root/qml" "${files[@]}" 2>&1)" || {
  echo "$output"
  exit 1
}
echo "$output"
if echo "$output" | grep -Eqi 'warning|error'; then
  echo "qmllint reported warnings or errors" >&2
  exit 1
fi
