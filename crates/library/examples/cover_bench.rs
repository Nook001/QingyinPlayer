use std::env;
use std::io::Cursor;
use std::time::Instant;

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use qingyin_library::{CoverLookup, CoverService};
use qingyin_metadata::{CoverArt, FileFingerprint, TrackMetadata};

fn main() {
    let count = env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1000);
    let viewport = env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(40);
    let work = env::temp_dir().join(format!(
        "qingyin-cover-bench-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let service = CoverService::open(&work).expect("cover cache");
    let art = CoverArt {
        data: tiny_png(),
        extension: "png".into(),
    };
    let tracks = (0..count)
        .map(|index| {
            let mut track = TrackMetadata::from_display(
                format!("/tmp/qingyin-cover-bench/{index:05}.wav"),
                format!("曲目 {index}"),
                Some("专辑".into()),
                vec!["歌手".into()],
                None,
            );
            track.file_size = 64;
            track.modified_at_ns = 1_000 + i64::from(index);
            track
        })
        .collect::<Vec<_>>();

    let origin = Instant::now();
    let mut origin_writes = 0usize;
    for track in &tracks {
        let url = service
            .store(
                &track.path,
                FileFingerprint {
                    modified_at_ns: track.modified_at_ns,
                    file_size: track.file_size,
                },
                &art,
            )
            .expect("store");
        if url.is_some() {
            origin_writes += 1;
        }
    }
    let origin_ms = origin.elapsed().as_secs_f64() * 1000.0;

    let cold_lookup = Instant::now();
    let mut viewport_hits = 0usize;
    for track in tracks.iter().take(viewport) {
        if matches!(service.lookup(track), CoverLookup::Ready(_)) {
            viewport_hits += 1;
        }
    }
    let viewport_ms = cold_lookup.elapsed().as_secs_f64() * 1000.0;

    let full = Instant::now();
    let mut full_hits = 0usize;
    for track in &tracks {
        if matches!(service.lookup(track), CoverLookup::Ready(_)) {
            full_hits += 1;
        }
    }
    let full_ms = full.elapsed().as_secs_f64() * 1000.0;

    println!(
        "tracks={count} viewport={viewport} origin_writes={origin_writes} origin_ms={origin_ms:.2} viewport_hits={viewport_hits} viewport_ms={viewport_ms:.2} full_hits={full_hits} full_ms={full_ms:.2} dpr_note=cache_is_512px_png;_1x_and_2x_sourceSize_decode_cached_file_without_rereading_audio rss_kb={}",
        peak_rss_kb().unwrap_or(0)
    );
    let _ = std::fs::remove_dir_all(work);
}

fn tiny_png() -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(32, 32, Rgb([36u8, 116, 95])));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode png");
    bytes
}

fn peak_rss_kb() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("VmHWM:") {
            return value.split_whitespace().next()?.parse().ok();
        }
    }
    None
}
