use qmetaobject::QmlEngine;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    qingyin_ui_bridge::register_qml_types();

    let mut engine = QmlEngine::new();
    let qml_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../qml/Main.qml");
    info!(qml_path, "loading QML application");
    engine.load_file(qml_path.into());
    engine.exec();
}
