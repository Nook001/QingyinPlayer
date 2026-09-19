fn main() {
    let qt_include_path = std::env::var("DEP_QT_INCLUDE_PATH").expect("DEP_QT_INCLUDE_PATH");
    let qt_version = std::env::var("DEP_QT_VERSION").expect("DEP_QT_VERSION");

    let mut build = cc::Build::new();
    build.cpp(true).file("cpp/pointer_guard.cpp");
    if let Ok(flags) = std::env::var("DEP_QT_COMPILE_FLAGS") {
        for flag in flags.split_terminator(';') {
            if !flag.is_empty() {
                build.flag(flag);
            }
        }
    }
    build.include(&qt_include_path);
    for module in ["QtCore", "QtGui", "QtQml", "QtQuick"] {
        build.include(format!("{qt_include_path}/{module}"));
        build.include(format!("{qt_include_path}/{module}/{qt_version}"));
        build.include(format!("{qt_include_path}/{module}/{qt_version}/{module}"));
    }
    build.compile("qingyin_pointer_guard");

    println!("cargo:rerun-if-changed=cpp/pointer_guard.cpp");
    println!("cargo:rerun-if-changed=cpp/pointer_guard.hpp");
}
