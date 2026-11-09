#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use log::info;
    use vector_field_explorer::VfApp;
    use vector_field_explorer::version::VERSION;
    use vector_field_explorer::window::{WINDOW_TITLE, window_options};

    // Ticket 20: `RUST_LOG` selects the filter, defaulting to `info`; logs go
    // to stderr. The wasm build initializes `console_log` in its own entry
    // point.
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
fn main() {}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast as _;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), wasm_bindgen::JsValue> {
    console_log::init_with_level(log::Level::Info).ok();
    match start_app().await {
        Ok(()) => Ok(()),
        Err(error) => {
            show_startup_error(&error);
            Err(error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn start_app() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("browser window unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("browser document unavailable"))?;
    let canvas = document
        .get_element_by_id("vfe-canvas")
        .ok_or_else(|| JsValue::from_str("vfe-canvas is missing"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(JsValue::from)?;

    log::info!(
        "{} v{} starting (wasm)",
        vector_field_explorer::window::WINDOW_TITLE,
        vector_field_explorer::version::VERSION
    );
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| {
                Ok(Box::new(vector_field_explorer::VfApp::new_with_storage(cc.storage)))
            }),
        )
        .await?;

    if let Some(loading) = document.get_element_by_id("loading") {
        loading.remove();
    }
    Ok(())
}

/// Replace the loading overlay with a readable startup error. Ticket 54 owns
/// the full WebGPU-unavailable guidance; this is the generic failure path.
#[cfg(target_arch = "wasm32")]
fn show_startup_error(error: &JsValue) {
    let message = error
        .as_string()
        .unwrap_or_else(|| format!("{error:?}"));
    log::error!("Vector Field Explorer failed to start: {message}");
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(loading) = document
        .get_element_by_id("loading")
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    loading.set_inner_text(&format!(
        "Vector Field Explorer could not start: {message}\n\nCheck the browser console for details."
    ));
}
