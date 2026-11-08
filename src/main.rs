#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use log::info;
    use vector_field_explorer::VfApp;
    use vector_field_explorer::version::VERSION;
    use vector_field_explorer::window::{WINDOW_TITLE, window_options};

    // Ticket 20: `RUST_LOG` selects the filter, defaulting to `info`; logs go
    // to stderr. The wasm build initializes `console_log` in its own entry
    // point (ticket 16).
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // The build version seam (ticket 42): a nightly stamps its full version
    // through `VFE_BUILD_VERSION`, so the startup line matches About.
    info!("{WINDOW_TITLE} v{VERSION} starting (native)");
    let options = window_options();
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(VfApp::new_with_storage(cc.storage)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // The wasm entry point (`WebRunner` + `console_log` at `info`) lands in
    // ticket 16; the target-gated dependency seam exists now.
}
