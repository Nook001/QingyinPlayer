use std::env;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use qingyin_player::{Player, PlayerEvent};

fn main() {
    unsafe {
        env::set_var("QINGYIN_AUDIO_SINK", "fakesink");
    }
    let work = env::temp_dir().join(format!(
        "qingyin-progress-bench-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&work).expect("work");
    let path = work.join("tone.wav");
    write_wav(&path, Duration::from_secs(8));

    let (tx, rx) = mpsc::channel();
    let mut player = Player::spawn(move |event| {
        let _ = tx.send(event);
    });
    wait_for(&rx, |event| matches!(event, PlayerEvent::Ready));
    player.mark_ready();
    let _ = player.load(&path);
    wait_for(&rx, |event| {
        matches!(
            event,
            PlayerEvent::CommandFinished {
                kind: qingyin_player::CommandKind::Load,
                error: None,
                ..
            }
        )
    });
    let _ = player.play();
    wait_for(&rx, |event| {
        matches!(
            event,
            PlayerEvent::StateChanged {
                state: qingyin_player::PlaybackState::Playing,
                ..
            } | PlayerEvent::Progress { .. }
        )
    });

    let visible = count_progress(&rx, Duration::from_secs(2));
    player.set_ui_visible(false);
    let hidden = count_progress(&rx, Duration::from_secs(4));
    let _ = player.pause();
    let paused = count_progress(&rx, Duration::from_secs(1));
    let shutdown = Instant::now();
    player.shutdown().expect("shutdown");
    let shutdown_ms = shutdown.elapsed().as_secs_f64() * 1000.0;

    println!(
        "visible_progress={visible} hidden_progress={hidden} paused_progress={paused} shutdown_ms={shutdown_ms:.2} rss_kb={}",
        peak_rss_kb().unwrap_or(0)
    );
    let _ = std::fs::remove_dir_all(work);
}

fn count_progress(rx: &mpsc::Receiver<PlayerEvent>, window: Duration) -> usize {
    let deadline = Instant::now() + window;
    let mut count = 0usize;
    while Instant::now() < deadline {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(PlayerEvent::Progress { .. }) => count += 1,
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    count
}

fn wait_for(rx: &mpsc::Receiver<PlayerEvent>, mut matches: impl FnMut(&PlayerEvent) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(event) if matches(&event) => return,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    panic!("timed out waiting for player event");
}

fn write_wav(path: &Path, duration: Duration) {
    let frames = 8_000u32 * u32::try_from(duration.as_secs()).expect("secs");
    let data_len = frames.saturating_mul(2);
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + data_len).to_le_bytes());
    bytes.extend(b"WAVE");
    bytes.extend(b"fmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(8000u32.to_le_bytes());
    bytes.extend(16000u32.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_len.to_le_bytes());
    bytes.extend(vec![0_u8; data_len as usize]);
    std::fs::write(path, bytes).expect("wav");
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
