fn main() {
    let include = std::env::var("DEP_QT_INCLUDE_PATH").expect("Qt include path from qttypes");
    let mut config = cpp_build::Config::new();
    config.include(&include);
    if let Ok(flags) = std::env::var("DEP_QT_COMPILE_FLAGS") {
        for flag in flags.split(';').filter(|flag| !flag.is_empty()) {
            config.flag(flag);
        }
    }
    config.build("src/main.rs");
}
