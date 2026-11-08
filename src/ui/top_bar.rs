//! The top menu bar: View / Settings / Help plus the screenshot action at the
//! right end (ticket 14). It replaces the old style + export blocks that used
//! to live in the left sidebar: appearance moved into a Settings popover and
//! the screenshot button now sits in this row.
//!
//! The bar only reports what the user asked for ([`TopAction`]); the app owns
//! the state each action mutates, so every wiring decision stays in one place.

use eframe::egui;

/// One point of wheel scroll zooms by `exp(rate * points)`: about 10.5% per
/// typical 50-point notch, and the reciprocal when scrolling the other way.
pub const WHEEL_ZOOM_RATE: f32 = 0.002;

/// What the user picked from the top bar this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopAction {
    ToggleControls,
    TogglePresets,
    ResetLayout,
    ResetCamera,
    ToggleHelp,
    Screenshot,
}

/// The pure part of the wheel zoom: map wheel points to a multiplicative zoom
/// factor (`> 1` = closer). A negative delta zooms out, and a nonsense delta
/// leaves the view alone.
pub fn wheel_zoom_factor(scroll_points: f32) -> f32 {
    if !scroll_points.is_finite() {
        return 1.0;
    }
    let factor = (scroll_points * WHEEL_ZOOM_RATE).exp();
    if factor.is_finite() && factor > 0.0 {
        factor
    } else {
        1.0
    }
}

/// This frame's viewport zoom factor: the pinch/ctrl-scroll delta egui reports
/// times the plain-wheel delta (egui routes ctrl-scroll into `zoom_delta`, so
/// the two never double-count the same wheel notch).
pub fn wheel_zoom(ctx: &egui::Context) -> f32 {
    ctx.input(|i| {
        let pinch = i.zoom_delta();
        let pinch = if pinch.is_finite() && pinch > 0.0 { pinch } else { 1.0 };
        let factor = pinch * wheel_zoom_factor(i.smooth_scroll_delta.y);
        if factor.is_finite() && factor > 0.0 { factor } else { 1.0 }
    })
}

/// Keyboard shortcut labels shown in the View menu and the rail tooltips.
pub const CONTROLS_SHORTCUT: &str = "Ctrl+B";
/// Keyboard shortcut for the Presets panel.
pub const PRESETS_SHORTCUT: &str = "Ctrl+P";

/// Draw the menu row. Returns the action the user picked (if any) and whether
/// the Settings popover changed the appearance record (the app re-applies it
/// in the same frame).
pub fn show(
    ui: &mut egui::Ui,
    layout: &mut crate::ui::layout::LayoutState,
    style: &mut crate::ui::style_panel::AppStyle,
    screenshot: &mut crate::screenshot::Screenshot,
) -> (Option<TopAction>, bool) {
    let mut action = None;
    let mut style_changed = false;

    ui.horizontal(|ui| {
        ui.menu_button("View", |ui| {
            if ui
                .button(format!("Controls panel ({CONTROLS_SHORTCUT})"))
                .clicked()
            {
                action = Some(TopAction::ToggleControls);
                ui.close();
            }
            if ui
                .button(format!("Presets panel ({PRESETS_SHORTCUT})"))
                .clicked()
            {
                action = Some(TopAction::TogglePresets);
                ui.close();
            }
            ui.separator();
            if ui.button("Reset layout").clicked() {
                action = Some(TopAction::ResetLayout);
                ui.close();
            }
            if ui
                .button("Reset camera")
                .on_hover_text("Return the camera to its starting viewpoint")
                .clicked()
            {
                action = Some(TopAction::ResetCamera);
                ui.close();
            }
        });

        // Non-modal popover: menu buttons are exactly that in egui, and the
        // viewport stays visible while the appearance sliders move.
        ui.menu_button("Settings", |ui| {
            ui.label(egui::RichText::new("Appearance").strong());
            style_changed |= crate::ui::style_panel::show_controls(ui, style);

            ui.separator();
            let mut help = layout.help_visible;
            if ui.checkbox(&mut help, "Show help overlay").changed() {
                layout.help_visible = help;
            }
            if ui.button("Reset layout").clicked() {
                action = Some(TopAction::ResetLayout);
                ui.close();
            }
        });

        if ui
            .selectable_label(layout.help_visible, "Help")
            .on_hover_text("Show or hide the mouse/formula help overlay")
            .clicked()
        {
            action = Some(TopAction::ToggleHelp);
        }

        // Screenshot at the right end of the row; its completion toast sits
        // just left of the button.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Screenshot")
                .on_hover_text("Save the viewport exactly as it looks now")
                .clicked()
            {
                action = Some(TopAction::Screenshot);
                screenshot.request();
            }
            screenshot.toast(ui);
        });
    });

    (action, style_changed)
}
