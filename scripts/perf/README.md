# Performance measurement

Scripts write isolated fixtures under `$TMPDIR/qingyin-perf-*` and results under
`scripts/perf/results/`. They never touch the user music library or
`~/.config/qingyin`.

Sizes: `1000`, `10000`, `50000`.

```bash
bash scripts/perf/run.sh 1000          # PERF-01/02 scan + search
bash scripts/perf/run.sh 1000 rest     # PERF-03..08
```

Spans: `library.scan`, `covers.fulfill`, `ui.apply_snapshot`. Enable with
`RUST_LOG=qingyin=info,qingyin_library=info`.

## 2026-09-20 1k sample (this machine)

- Scan 1000 tiny WAV files: cold ~58 ms, hot ~7.5 ms, 1000 imported then 1000 unchanged.
  Peak RSS ~8 MB. Production scan is serial; a 4-worker parse-only experiment was
  2.4 ms and did not change SQLite or the 32 MiB in-flight cover budget. Keep
  serial import.
- Search 10k synthetic rows, query `清音`: ~10.6 ms/query; plan is
  `SCAN search_terms` plus `SEARCH tracks USING INTEGER PRIMARY KEY`.
- Decision for PERF-02: keep the production `instr` substring SQL and current
  `search_terms` indexes. Additional covering indexes were not adopted because
  the measured plan already seeks tracks by primary key after the term scan,
  and changing to equality SQL would drop middle-substring semantics.
- Cover cache: 1000 origin stores ~5.5 s; 40 viewport lookups 0.53 ms; 1000
  cache hits 12.9 ms. 1×/2× `sourceSize` buckets (44/88 at display 44) decode
  the 512 px cache file and do not reread audio.
- Virtualized list: 2000 model rows, 22 live delegates (offscreen qmltestrunner).
  No extra role/layout change item from this run.
- Playback progress: visible ~8 ticks / 2 s, hidden ~2 ticks / 4 s, pause at most
  one in-flight tick, shutdown ~1 ms.
- Stress: exclusive SQLite lock fails after the 5 s busy timeout; after unlock
  the 1000-track library is unchanged; remount requests reconcile; watcher stop
  ~0.1 ms.
- Release A/B: default 11.8 MB / ~89 MB RSS; thin LTO + cgu=1 + strip 8.8 MB /
  ~89 MB RSS. Smoke CPU unchanged. Keep the default release profile.
