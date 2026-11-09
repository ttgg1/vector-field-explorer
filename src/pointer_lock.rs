//! Web Pointer Lock bridge and the native raw-motion seam (ticket 65).
//!
//! eframe's web backend implements neither `CursorGrab(Locked)` nor raw
//! [`egui::Event::MouseMoved`] deltas, so the wasm build calls the classic
//! script in `web/pointer_lock.js` through a tiny `js_sys::Reflect` bridge —
//! no new egui input channel. The script owns `pointerlockchange` tracking and
//! accumulates `movementX/Y`; Rust drains it exactly once per frame.
//!
//! Native platforms use winit's raw motion, which eframe forwards as
//! `MouseMoved` deltas in physical-ish pixels; [`motion_delta`] divides by
//! `pixels_per_point` so the fly camera's 0.005 rad/point sensitivity matches
//! drag-look. Everything here is best-effort: a missing bridge or a refused
//! lock leaves fly usable with drag-look.

use eframe::egui;
use glam::Vec2;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast as _;

/// The `window.vfePointerLock` function called `name`, when the bridge script
/// is present and exposes it.
#[cfg(target_arch = "wasm32")]
fn bridge_function(name: &str) -> Option<js_sys::Function> {
    use wasm_bindgen::JsValue;

    let window = web_sys::window()?;
    let target: &JsValue = window.as_ref();
    let bridge = js_sys::Reflect::get(target, &JsValue::from_str("vfePointerLock")).ok()?;
    if !bridge.is_object() {
        return None;
    }
    let function = js_sys::Reflect::get(&bridge, &JsValue::from_str(name)).ok()?;
    function.dyn_into::<js_sys::Function>().ok()
}

/// Ask the browser for pointer lock. Call once on entering fly; failure is
/// silent and leaves drag-look in charge.
pub fn request() {
    #[cfg(target_arch = "wasm32")]
    if let Some(function) = bridge_function("request") {
        let _ = function.call0(&wasm_bindgen::JsValue::NULL);
    }
}

/// Release pointer lock and discard any accumulated motion. No-op natively.
pub fn release() {
    #[cfg(target_arch = "wasm32")]
    if let Some(function) = bridge_function("release") {
        let _ = function.call0(&wasm_bindgen::JsValue::NULL);
        let _ = take_delta();
    }
}

/// Drain the accumulated pointer-lock motion in points. The wasm bridge zeroes
/// its accumulator here; native has no bridge and returns zero.
pub fn take_delta() -> Vec2 {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;

        if let Some(function) = bridge_function("takeDelta")
            && let Ok(value) = function.call0(&JsValue::NULL)
        {
            let array = js_sys::Array::from(&value);
            return Vec2::new(finite_component(&array, 0), finite_component(&array, 1));
        }
    }
    Vec2::ZERO
}

/// Whether pointer lock was acquired and has since dropped. The browser Esc
/// that releases the lock is normally reserved (it never reaches egui), so the
/// app treats this as the web Esc: leave fly entirely. An initial refusal
/// (`lockLost` never fired) is not a loss and never blocks fly.
pub fn lock_lost() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;

        if let Some(function) = bridge_function("lockLost")
            && let Ok(value) = function.call0(&JsValue::NULL)
        {
            return value.is_truthy();
        }
    }
    false
}

/// This frame's raw look delta in points. Native sums `MouseMoved` events and
/// scales physical pixels to points; wasm drains the pointer-lock bridge.
/// Call only while captured, and exactly once per frame.
pub fn motion_delta(ctx: &egui::Context) -> Vec2 {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        take_delta()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let raw = ctx.input(|input| {
            input.events.iter().fold(Vec2::ZERO, |sum, event| match event {
                egui::Event::MouseMoved(delta) => sum + Vec2::new(delta.x, delta.y),
                _ => sum,
            })
        });
        let pixels_per_point = ctx.pixels_per_point();
        let points = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            raw / pixels_per_point
        } else {
            raw
        };
        Vec2::new(finite(points.x), finite(points.y))
    }
}

#[cfg(target_arch = "wasm32")]
fn finite_component(array: &js_sys::Array, index: u32) -> f32 {
    let value = array.get(index).as_f64().unwrap_or(0.0) as f32;
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(not(target_arch = "wasm32"))]
fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}
