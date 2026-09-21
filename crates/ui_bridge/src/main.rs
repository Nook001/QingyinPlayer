use qmetaobject::prelude::*;
use qmetaobject::{CompilationMode, ComponentStatus, QString, QUrl, QmlComponent, qrc};
use tracing::info;
use tracing_subscriber::EnvFilter;

qrc!(embedded_qml,
    "../../qml" as "qml" {
        "Main.qml",
        "Library.qml",
        "LibraryViewSwitch.qml",
        "Artist.qml",
        "Album.qml",
        "Settings.qml",
        "PlayerBar.qml",
        "NowPlaying.qml",
        "AudioDetails.qml",
        "TrackTable.qml",
        "PageScrollBar.qml",
        "CollectionBrowser.qml",
        "CoverImage.qml",
        "Icon.qml",
        "RoundedRect.qml",
        "FlatButton.qml",
        "AccentButton.qml",
        "Theme.qml",
        "Qingyin/qmldir",
        "Qingyin/plugins.qmltypes",
    }
);

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Union 风格的 Slider 会和我们的绑定打架；Basic 避开这个问题。
    // SAFETY: Qt 尚未创建线程或 QApplication。
    unsafe {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
        if std::env::var_os("QSG_SAMPLES").is_none() {
            std::env::set_var("QSG_SAMPLES", "4");
        }
    }

    embedded_qml();
    qingyin_ui_bridge::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path("qrc:/qml".into());
    if !qml_root_ready(&engine, "qrc:/qml/Main.qml") {
        eprintln!("failed to load QML root from qrc:/qml/Main.qml");
        std::process::exit(1);
    }
    engine.load_file("qrc:/qml/Main.qml".into());
    if std::env::args().any(|argument| argument == "--smoke") {
        info!("QML loaded, smoke ok");
        return;
    }
    info!("QML loaded, entering event loop");
    engine.exec();
}

fn qml_root_ready(engine: &QmlEngine, url: &str) -> bool {
    let mut component = QmlComponent::new(engine);
    component.load_url(
        QUrl::from(QString::from(url)),
        CompilationMode::PreferSynchronous,
    );
    component.status() == ComponentStatus::Ready
}
