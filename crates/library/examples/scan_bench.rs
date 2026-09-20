use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use qingyin_library::{ScanEvent, scan_directory, scan_directory_with};
use qingyin_metadata::read_tagged_track;
use qingyin_storage::Database;

fn main() {
    let directory = PathBuf::from(
        env::args()
            .nth(1)
            .expect("usage: scan_bench DIRECTORY [workers]"),
    );
    let workers = env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let db_path = env::temp_dir().join(format!(
        "qingyin-scan-bench-{}-{}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut database = Database::open(&db_path).expect("open database");
    let mut cold_parsed = 0usize;
    let mut cold_covers = 0usize;
    let cold = Instant::now();
    let summary = scan_directory_with(&mut database, &directory, |event| match event {
        ScanEvent::Imported { cover, .. } => {
            cold_parsed += 1;
            if cover.is_some() {
                cold_covers += 1;
            }
        }
        ScanEvent::Unchanged { .. } => {}
    })
    .expect("cold scan");
    let cold_ms = cold.elapsed().as_secs_f64() * 1000.0;
    let cold_rss = peak_rss_kb();

    let hot = Instant::now();
    let hot_summary = scan_directory(&mut database, &directory).expect("hot scan");
    let hot_ms = hot.elapsed().as_secs_f64() * 1000.0;
    let hot_rss = peak_rss_kb();

    println!(
        "mode=serial cold_ms={cold_ms:.2} hot_ms={hot_ms:.2} discovered={} imported={} unchanged={} failed={} parsed={} covers={} upsert_batches={} cold_rss_kb={} hot_rss_kb={}",
        summary.discovered,
        summary.imported,
        hot_summary.unchanged,
        summary.failed.len(),
        cold_parsed,
        cold_covers,
        summary.imported.div_ceil(50),
        cold_rss.unwrap_or(0),
        hot_rss.unwrap_or(0)
    );

    if workers > 0 {
        let paths = collect_audio(&directory);
        let started = Instant::now();
        let (parsed, covers) = parallel_parse(&paths, workers);
        println!(
            "mode=parallel workers={workers} parse_ms={:.2} parsed={parsed} covers={covers} note=parse_only sqlite_still_serial",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }

    let _ = fs::remove_file(db_path);
}

fn parallel_parse(paths: &[PathBuf], workers: usize) -> (usize, usize) {
    let workers = workers.clamp(1, 8);
    let chunk = paths.len().div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        let mut joins = Vec::new();
        for slice in paths.chunks(chunk) {
            joins.push(scope.spawn(move || {
                let mut parsed = 0usize;
                let mut covers = 0usize;
                let mut inflight = 0usize;
                for path in slice {
                    if let Ok((_, cover)) = read_tagged_track(path) {
                        parsed += 1;
                        if let Some(art) = cover {
                            inflight = inflight.saturating_add(art.data.len());
                            covers += 1;
                            if inflight > 32 * 1024 * 1024 {
                                inflight = 0;
                            }
                        }
                    }
                }
                (parsed, covers)
            }));
        }
        joins.into_iter().fold((0, 0), |(parsed, covers), join| {
            let (next_parsed, next_covers) = join.join().expect("worker");
            (parsed + next_parsed, covers + next_covers)
        })
    })
}

fn collect_audio(directory: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_audio_into(directory, &mut paths);
    paths.sort();
    paths
}

fn collect_audio_into(directory: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_audio_into(&path, paths);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "wav" | "flac" | "mp3" | "ogg" | "m4a" | "opus"
                )
            })
        {
            paths.push(path);
        }
    }
}

fn peak_rss_kb() -> Option<u64> {
    let text = fs::read_to_string("/proc/self/status").ok()?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("VmHWM:") {
            return value.split_whitespace().next()?.parse().ok();
        }
    }
    None
}
