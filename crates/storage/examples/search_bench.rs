use std::env;
use std::path::PathBuf;
use std::time::Instant;

use qingyin_metadata::TrackMetadata;
use qingyin_storage::Database;

fn main() {
    let query = env::args().nth(1).unwrap_or_else(|| "qingyin".into());
    let mut database = Database::open_in_memory().expect("open");
    for index in 0..10_000 {
        let mut track = TrackMetadata::from_display(
            format!("/music/{index:05}.wav"),
            if index % 17 == 0 { "清音" } else { "其他" },
            Some("专辑".into()),
            vec!["歌手".into()],
            None,
        );
        track.id = 0;
        database.upsert_track(&track).expect("upsert");
    }
    let roots = [PathBuf::from("/music")];
    let start = Instant::now();
    let mut last = 0;
    for _ in 0..50 {
        last = database
            .search_tracks(&query, 50, &roots)
            .expect("search")
            .tracks
            .len();
    }
    let elapsed = start.elapsed();
    let plan = database.explain_search_plan(&query, &roots).expect("plan");
    println!(
        "query={query} hits={last} p50_like_ms={:.3} plan={plan}",
        elapsed.as_secs_f64() * 1000.0 / 50.0
    );
}
