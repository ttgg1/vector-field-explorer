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

/// Check WebGPU availability before eframe initializes: read `navigator.gpu`
/// and await `requestAdapter()` with default options. `Err(reason)` is the
/// user-facing explanation shown in the help panel; `Ok(())` means an adapter
/// exists (eframe requests its own). This stays textually above the runner
/// construction so the gate cannot be silently moved after the app starts.
#[cfg(target_arch = "wasm32")]
async fn ensure_webgpu(window: &web_sys::Window) -> Result<(), String> {
    // `navigator.gpu` is absent in browsers without WebGPU; the web-sys getter
    // yields `undefined` for that property. Check it as a JS value before
    // treating it as a `Gpu` instance.
    let gpu = window.navigator().gpu();
    let gpu_value: JsValue = gpu.clone().into();
    if gpu_value.is_undefined() || gpu_value.is_null() {
        return Err("navigator.gpu is unavailable in this browser".to_string());
    }

    let options = web_sys::GpuRequestAdapterOptions::new();
    match wasm_bindgen_futures::JsFuture::from(gpu.request_adapter_with_options(&options)).await {
        Ok(adapter) if !adapter.is_empty() => {
            log::info!("WebGPU adapter acquired");
            Ok(())
        }
        Ok(_) => Err("requestAdapter() returned no adapter".to_string()),
        Err(error) => Err(format!("requestAdapter() failed: {error:?}")),
    }
}

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

    // Ticket 54 pre-start gate: navigator.gpu must exist and requestAdapter()
    // must yield an adapter before WebRunner is constructed. An unsupported
    // browser sees the static help panel (with the reason) instead of eframe
    // failing into a blank canvas. There is deliberately no WebGL2 fallback.
    if let Err(reason) = ensure_webgpu(&window).await {
        show_webgpu_unsupported(&reason);
        return Ok(());
    }

    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("browser document unavailable"))?;
    hide_webgpu_panel(&document);
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

/// Show the static `#webgpu-unsupported` panel with the failure reason. The
/// DOM is toggled directly as well as through `web/webgpu.js`, so a blocked or
/// late helper script cannot leave a blank canvas.
#[cfg(target_arch = "wasm32")]
fn show_webgpu_unsupported(reason: &str) {
    log::warn!("WebGPU unavailable: {reason}");
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    reveal_webgpu_panel(&document, reason);
    // Adds the browser-specific support link on top of the toggle above.
    let _ = call_vfe_webgpu("show", reason);
}

#[cfg(target_arch = "wasm32")]
fn reveal_webgpu_panel(document: &web_sys::Document, reason: &str) {
    if let Some(panel) = document.get_element_by_id("webgpu-unsupported") {
        let _ = panel.remove_attribute("hidden");
    }
    if let Some(reason_line) = document.get_element_by_id("webgpu-reason") {
        let _ = reason_line.remove_attribute("hidden");
    }
    if let Some(code) = document
        .get_element_by_id("webgpu-reason-code")
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        code.set_inner_text(reason);
    }
    if let Some(body) = document.body() {
        let _ = body.set_attribute("class", "webgpu-unsupported");
    }
}

#[cfg(target_arch = "wasm32")]
fn hide_webgpu_panel(document: &web_sys::Document) {
    if let Some(panel) = document.get_element_by_id("webgpu-unsupported") {
        let _ = panel.set_attribute("hidden", "");
    }
    if let Some(body) = document.body() {
        let _ = body.remove_attribute("class");
    }
}

/// Call `window.vfeWebGpu.<function>(reason)` when `web/webgpu.js` has loaded.
/// Returns whether the call was made; the caller never depends on it.
#[cfg(target_arch = "wasm32")]
fn call_vfe_webgpu(function: &str, reason: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let target: &JsValue = window.as_ref();
    let Ok(api) = js_sys::Reflect::get(target, &JsValue::from_str("vfeWebGpu")) else {
        return false;
    };
    if !api.is_object() {
        return false;
    }
    let Ok(candidate) = js_sys::Reflect::get(&api, &JsValue::from_str(function)) else {
        return false;
    };
    let Ok(callback) = candidate.dyn_into::<js_sys::Function>() else {
        return false;
    };
    callback
        .call1(&JsValue::NULL, &JsValue::from_str(reason))
        .is_ok()
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
