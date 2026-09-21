//! Exercises the real Rust bridge, local scan, lyrics worker, and GStreamer session.
use qmetaobject::{QByteArray, QString, QUrl, QmlEngine};

#[test]
fn local_lyrics_and_audio_info_share_the_playback_session() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let directory =
        std::env::temp_dir().join(format!("qingyin-now-playing-{}", std::process::id()));
    let music = directory.join("music");
    std::fs::create_dir_all(&music).unwrap();
    // This integration test is the only test in its process; configure Qt before spawning threads.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
        std::env::set_var("QINGYIN_AUDIO_SINK", "fakesink");
        for (name, subdir) in [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
        ] {
            std::env::set_var(name, directory.join(subdir));
        }
    }
    let mut wav = qingyin_metadata::test_support::silence_wav_bytes();
    wav.resize(44 + 8000 * 2 * 30, 0);
    let data_size = (wav.len() - 44) as u32;
    wav[4..8].copy_from_slice(&(36 + data_size).to_le_bytes());
    wav[40..44].copy_from_slice(&data_size.to_le_bytes());
    std::fs::write(music.join("夜曲.wav"), wav).unwrap();
    std::fs::write(
        music.join("夜曲.lrc"),
        "[00:00.00]晚风吹过安静的街道\n[00:01.00]灯光映在你的眼里\n[00:10.00]让这一首歌慢慢播放",
    )
    .unwrap();
    let settings = qingyin_core::Settings {
        music_directories: vec![music],
        ..Default::default()
    };
    settings.save().unwrap();
    qingyin_ui_bridge::register_qml_types();
    let mut engine = QmlEngine::new();
    let qml_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../qml")
        .canonicalize()
        .unwrap();
    engine.add_import_path(QString::from(qml_dir.to_string_lossy().as_ref()));
    engine.load_data_as(
        QByteArray::from(include_str!("../../../tests/qml/now_playing.qml")),
        QUrl::from(QString::from(format!(
            "file://{}/integration.qml",
            qml_dir.display()
        ))),
    );
    assert!(
        engine.invoke_method("testResult".into(), &[]).is_valid(),
        "QML harness did not load"
    );
    engine.exec();
    assert_eq!(
        engine
            .invoke_method("testResult".into(), &[])
            .to_qstring()
            .to_string(),
        "ok"
    );
    drop(engine);
    std::fs::remove_dir_all(directory).unwrap();
}
