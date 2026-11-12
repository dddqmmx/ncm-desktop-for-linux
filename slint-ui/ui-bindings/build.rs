fn main() {
    // The live-preview runtime and generated Rust bindings must be enabled
    // together. Keep ordinary builds/tests on the compiled UI path.
    if std::env::var_os("CARGO_FEATURE_HOT_RELOAD").is_some() {
        std::env::set_var("SLINT_LIVE_PREVIEW", "1");
    }
    let config = slint_build::CompilerConfiguration::new().with_debug_info(true);
    slint_build::compile_with_config("../ui/app.slint", config)
        .expect("Failed to compile Slint UI");
}
