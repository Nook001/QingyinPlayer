use qmetaobject::QmlEngine;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Union 风格的 Slider 会和我们的绑定打架；Basic 避开这个问题。
    // SAFETY: Qt 尚未创建线程或 QApplication。
    unsafe {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
    }

    qingyin_ui_bridge::register_qml_types();

    let mut engine = QmlEngine::new();
    let qml_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../qml/Main.qml");
    info!(qml_path, "loading QML application");
    engine.load_file(qml_path.into());
    info!("QML loaded, entering event loop");
    engine.exec();
}
