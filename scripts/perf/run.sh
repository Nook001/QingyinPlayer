#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
size="${1:-1000}"
task="${2:-core}"
stamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
host="$(uname -a)"
work="$(mktemp -d "${TMPDIR:-/tmp}/qingyin-perf.XXXXXX")"
results="$root/scripts/perf/results"
mkdir -p "$results"
library="$work/music"
bash "$root/scripts/perf/generate_library.sh" "$size" "$library"

header() {
  echo "stamp=$stamp"
  echo "host=$host"
  echo "size=$size"
  echo "work=$work"
}

run_core() {
  local log="$results/scan-${size}.txt"
  {
    header
    echo "scan"
    cargo run -p qingyin-library --offline --example scan_bench -- "$library"
    echo "search_han"
    cargo run -p qingyin-storage --offline --example search_bench -- "清音"
    echo "search_pinyin"
    cargo run -p qingyin-storage --offline --example search_bench -- "qingyin"
    echo "search_initials"
    cargo run -p qingyin-storage --offline --example search_bench -- "qy"
    echo "search_latin"
    cargo run -p qingyin-storage --offline --example search_bench -- "in"
  } | tee "$log"
  echo "wrote $log"
}

run_scan() {
  local log="$results/scan-detail-${size}.txt"
  {
    header
    echo "serial_and_counts"
    cargo run -p qingyin-library --offline --example scan_bench -- "$library"
    echo "parallel_parse"
    cargo run -p qingyin-library --offline --example scan_bench -- "$library" 4
  } | tee "$log"
  echo "wrote $log"
}

run_cover() {
  local log="$results/cover-${size}.txt"
  {
    header
    echo "cover_cache"
    cargo run -p qingyin-library --offline --example cover_bench -- "$size" 40
    echo "dpr_buckets displaySize=44 1x=44 2x=88 cache_edge=512"
  } | tee "$log"
  echo "wrote $log"
}

run_play() {
  local log="$results/playback.txt"
  {
    header
    echo "progress_intervals"
    cargo run -p qingyin-player --offline --example progress_bench
  } | tee "$log"
  echo "wrote $log"
}

run_virtual() {
  local log="$results/virtualization.txt"
  local runner="${QMLTESTRUNNER:-}"
  if [[ -z "$runner" ]]; then
    for candidate in /usr/lib/qt6/bin/qmltestrunner qmltestrunner6 qmltestrunner; do
      if command -v "$candidate" >/dev/null 2>&1 || [[ -x "$candidate" ]]; then
        runner="$candidate"
        break
      fi
    done
  fi
  {
    header
    if [[ -z "$runner" ]]; then
      echo "qmltestrunner missing"
      exit 1
    fi
    export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}"
    export QT_QUICK_CONTROLS_STYLE="${QT_QUICK_CONTROLS_STYLE:-Basic}"
    "$runner" -input "$root/qml/tst_virtualization.qml" -import "$root/qml"
  } | tee "$log"
  echo "wrote $log"
}

run_stress() {
  local log="$results/stress-${size}.txt"
  {
    header
    echo "import_lock_remount"
    cargo run -p qingyin-library --offline --example stress_bench -- "$library"
  } | tee "$log"
  echo "wrote $log"
}

run_lto() {
  local log="$results/lto.txt"
  local lto_dir="$work/target-lto"
  local default_bin="$root/target/release/qingyin"
  {
    header
    echo "default_release"
    if [[ ! -x "$default_bin" ]]; then
      cargo build --release -p qingyin-ui-bridge --bin qingyin --offline
    fi
    echo "default_bin_bytes=$(stat -c %s "$default_bin")"
    echo -n "default_smoke "
    python3 - "$default_bin" <<'PY'
import os, resource, subprocess, sys
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_QUICK_CONTROLS_STYLE", "Basic")
completed = subprocess.run([sys.argv[1], "--smoke"], check=False)
usage = resource.getrusage(resource.RUSAGE_CHILDREN)
print(f"exit={completed.returncode} cpu_s={usage.ru_utime + usage.ru_stime:.3f} max_rss_kb={usage.ru_maxrss}")
raise SystemExit(completed.returncode)
PY
    echo "thin_lto_codegen_units_1_strip"
    local started
    started="$(date +%s)"
    CARGO_TARGET_DIR="$lto_dir" cargo build --release -p qingyin-ui-bridge --bin qingyin --offline \
      --config 'profile.release.lto="thin"' \
      --config 'profile.release.codegen-units=1' \
      --config 'profile.release.strip="symbols"'
    echo "lto_build_s=$(( $(date +%s) - started ))"
    echo "lto_bin_bytes=$(stat -c %s "$lto_dir/release/qingyin")"
    echo -n "lto_smoke "
    python3 - "$lto_dir/release/qingyin" <<'PY'
import os, resource, subprocess, sys
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_QUICK_CONTROLS_STYLE", "Basic")
completed = subprocess.run([sys.argv[1], "--smoke"], check=False)
usage = resource.getrusage(resource.RUSAGE_CHILDREN)
print(f"exit={completed.returncode} cpu_s={usage.ru_utime + usage.ru_stime:.3f} max_rss_kb={usage.ru_maxrss}")
raise SystemExit(completed.returncode)
PY
    echo "decision=keep_default_release_profile;_thin_lto_not_adopted_without_clear_startup_or_rss_win"
  } | tee "$log"
  echo "wrote $log"
}

case "$task" in
  core) run_core ;;
  scan) run_scan ;;
  cover) run_cover ;;
  play) run_play ;;
  virtual) run_virtual ;;
  stress) run_stress ;;
  lto) run_lto ;;
  rest)
    run_scan
    run_cover
    run_play
    run_virtual
    run_stress
    if [[ "${QINGYIN_SKIP_LTO:-}" != 1 ]]; then
      run_lto
    fi
    ;;
  all)
    run_core
    run_scan
    run_cover
    run_play
    run_virtual
    run_stress
    if [[ "${QINGYIN_SKIP_LTO:-}" != 1 ]]; then
      run_lto
    fi
    ;;
  *)
    echo "usage: $0 [size] [core|scan|cover|play|virtual|stress|lto|rest|all]" >&2
    exit 1
    ;;
esac

echo "fixture remains at $work until reboot/tmp cleanup"
