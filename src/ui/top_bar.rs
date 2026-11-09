//! Conventional top menu bar (ticket 15): File / Edit / View / Settings / Help
//! plus the screenshot action at the right end. It replaces ticket 14's
//! View/Settings/Help button row.
//!
//! The bar renders [`crate::ui::menu::MENUS`] and reports the picked
//! [`MenuAction`]; the app applies it through `menu::apply_action`, so menu
//! behaviour is testable without a window. The Settings popover and the About
//! dialog are the only stateful surfaces left here — they need egui anchoring
//! and therefore cannot live in the pure seam.

use eframe::egui;

use crate::screenshot::Screenshot;
use crate::ui::menu::{self, MenuAction, MENUS};
use crate::ui::style_panel::{self, AppStyle};

/// One point of wheel scroll zooms by `exp(rate * points)`: about 10.5% per
/// typical 50-point notch, and the reciprocal when scrolling the other way.
pub const WHEEL_ZOOM_RATE: f32 = 0.002;

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

/// Whether a click this frame dismisses the Appearance popover.
///
/// The popover owns a nested color-picker popup, which egui draws as a separate
/// floating area — often beyond the popover's own rect. `CloseOnClickOutside`
/// compares layers, so it treated every picker click as an outside click and
/// closed the popover mid-pick (ticket 41). A click on the popover, or on the
/// picker's own layer while it is open, is inside; any other click dismisses.
pub fn dismisses_appearance_popover(
    any_click: bool,
    click_pos: Option<egui::Pos2>,
    popover_rect: egui::Rect,
    layer_under_pointer: Option<egui::LayerId>,
    accent_popup_layer: Option<egui::LayerId>,
) -> bool {
    if !any_click {
        return false;
    }
    let Some(pos) = click_pos else {
        return false;
    };
    if popover_rect.contains(pos) {
        return false;
    }
    if accent_popup_layer.is_some() && accent_popup_layer == layer_under_pointer {
        return false;
    }
    true
}

/// Draw the menu bar. Returns the action the user picked (if any) and whether
/// the appearance popover changed the style record (the app re-applies it in
/// the same frame). `settings_open` is the popover's visibility flag, set by
/// the Settings > Appearance… or View > Appearance… item.
pub fn show(
    ui: &mut egui::Ui,
    style: &mut AppStyle,
    screenshot: &Screenshot,
    settings_open: &mut bool,
) -> (Option<MenuAction>, bool) {
    let mut action = None;
    let mut style_changed = false;
    let mut settings_button = None;

    egui::MenuBar::new().ui(ui, |ui| {
        for menu in MENUS {
            let response = ui.menu_button(menu.title, |ui| {
                for item in menu.items {
                    let mut button = egui::Button::new(item.label);
                    if let Some(label) = menu::item_shortcut_label(item) {
                        button = button.shortcut_text(label);
                    }
                    if ui.add(button).clicked() {
                        action = Some(item.action);
                        // Every item is a one-shot command: close the dropdown
                        // so the next click lands on the app, not the menu.
                        ui.close();
                    }
                }
            });
            if menu.title == menu::SETTINGS_TITLE {
                // The appearance popover anchors under the Settings title.
                settings_button = Some(response.response);
            }
        }

        // The screenshot action lives at the right end of the bar and
        // dispatches the same ExportScreenshot action as File > Export.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tooltip = match menu::shortcut_label(MenuAction::ExportScreenshot) {
                Some(label) => format!("Save the viewport exactly as it looks now ({label})"),
                None => "Save the viewport exactly as it looks now".to_string(),
            };
            if ui.button("Screenshot").on_hover_text(tooltip).clicked() {
                action = Some(MenuAction::ExportScreenshot);
            }
            screenshot.toast(ui);
        });
    });

    if let Some(button) = settings_button {
        let mut accent_popup_id = egui::Id::NULL;
        let response = egui::Popup::new(
            button.id.with("appearance_popover"),
            ui.ctx().clone(),
            &button,
            button.layer_id,
        )
        .anchor_widget(button.id)
        .open_bool(settings_open)
        // Dismissal is decided below, where the nested color picker's layer
        // can be excluded from "outside" (ticket 41).
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .show(|ui| {
            ui.set_width(260.0);
            ui.label(egui::RichText::new("Appearance").strong());
            let controls = style_panel::show_controls(ui, style);
            style_changed = controls.changed;
            accent_popup_id = controls.accent_popup_id;
        });

        if let Some(popover) = response {
            let accent_open = accent_popup_id != egui::Id::NULL
                && egui::Popup::is_id_open(ui.ctx(), accent_popup_id);
            let accent_layer = accent_open.then(|| {
                egui::LayerId::new(egui::Order::Foreground, accent_popup_id)
            });
            let (any_click, click_pos) =
                ui.ctx().input(|input| (input.pointer.any_click(), input.pointer.interact_pos()));
            let layer_under_pointer = click_pos.and_then(|pos| ui.ctx().layer_id_at(pos));
            if dismisses_appearance_popover(
                any_click,
                click_pos,
                popover.response.rect,
                layer_under_pointer,
                accent_layer,
            ) {
                *settings_open = false;
            }
        }
    }

    (action, style_changed)
}

/// The About dialog: name, package version, license and the repository link.
/// `open` is the visibility flag; the window's × clears it.
pub fn about_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new(format!("About {}", menu::ABOUT_NAME))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(menu::ABOUT_NAME).strong());
            ui.label(format!("Version {}", menu::ABOUT_VERSION));
            ui.label(format!("License: {}", menu::ABOUT_LICENSE));
            ui.hyperlink_to(menu::ABOUT_URL, menu::ABOUT_URL);
        });
}
