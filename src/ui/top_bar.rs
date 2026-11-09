//! Conventional top menu bar (ticket 15): File / Edit / View / Help, the
//! top-bar Appearance gear, and the screenshot action at the right end. It
//! replaces ticket 14's View/Settings/Help button row.
//!
//! The bar renders [`crate::ui::menu::MENUS`] and reports the picked
//! [`MenuAction`]; the app applies it through `menu::apply_action`, so menu
//! behaviour is testable without a window. The Appearance popover and the About
//! dialog are the only stateful surfaces left here — they need egui anchoring
//! and therefore cannot live in the pure seam.

use eframe::egui;

use crate::screenshot::Screenshot;
use crate::ui::branding;
use crate::ui::menu::{self, MenuAction, MENUS};
use crate::ui::style_panel::{self, AppStyle};

/// The gear button that anchors the Appearance popover. Ticket 59 replaced the
/// Settings menu (whose only item was Appearance…) with this button, leaving
/// `View > Appearance…` as the one menu command.
pub const APPEARANCE_BUTTON_LABEL: &str = "⚙";

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

/// The compact bar's hamburger button. It opens one flat dropdown with every
/// [`MENUS`] group, so the same commands stay one tap away on a phone.
pub const HAMBURGER_LABEL: &str = "☰";

/// Add one menu item and close the current dropdown when it is picked. Shared
/// by the wide menu bar and the compact hamburger so the two cannot drift.
fn add_menu_item(ui: &mut egui::Ui, item: &menu::MenuItem) -> bool {
    // Ticket 65: no menu item is a toggle any more; the fly control lives in
    // the sidebar Camera section.
    let mut button = egui::Button::new(item.label);
    if let Some(label) = menu::item_shortcut_label(item) {
        button = button.shortcut_text(label);
    }
    let clicked = ui.add(button).clicked();
    if clicked {
        // Every item is a one-shot command: close the dropdown so the next
        // click lands on the app, not the menu.
        ui.close();
    }
    clicked
}

/// The screenshot button shared by both bars. Returns whether it was clicked.
fn screenshot_button(ui: &mut egui::Ui, screenshot: &Screenshot) -> bool {
    let tooltip = match menu::shortcut_label(MenuAction::ExportScreenshot) {
        Some(label) => format!("Save the viewport exactly as it looks now ({label})"),
        None => "Save the viewport exactly as it looks now".to_string(),
    };
    let clicked = ui.button("Screenshot").on_hover_text(tooltip).clicked();
    screenshot.toast(ui);
    clicked
}

/// The Appearance popover anchored to the gear, including ticket 41's
/// color-picker dismissal rule. Returns whether the style record changed.
fn appearance_popover(
    ui: &mut egui::Ui,
    style: &mut AppStyle,
    button: &egui::Response,
    appearance_open: &mut bool,
) -> bool {
    let mut style_changed = false;
    let mut accent_popup_id = egui::Id::NULL;
    let response = egui::Popup::new(
        button.id.with("appearance_popover"),
        ui.ctx().clone(),
        button,
        button.layer_id,
    )
    .anchor_widget(button.id)
    .open_bool(appearance_open)
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
        let accent_layer =
            accent_open.then(|| egui::LayerId::new(egui::Order::Foreground, accent_popup_id));
        let (any_click, click_pos) =
            ui.ctx().input(|input| (input.pointer.any_click(), input.pointer.interact_pos()));
        let layer_under_pointer = click_pos.and_then(|pos| ui.ctx().layer_id_at(pos));
        // The gear click that opens the popover is outside its rect this
        // frame, so it must not immediately dismiss what it just opened.
        if !button.clicked()
            && dismisses_appearance_popover(
                any_click,
                click_pos,
                popover.response.rect,
                layer_under_pointer,
                accent_layer,
            )
        {
            *appearance_open = false;
        }
    }
    style_changed
}

/// Draw the menu bar. Returns the action the user picked (if any) and whether
/// the appearance popover changed the style record (the app re-applies it in
/// the same frame). `appearance_open` is the popover's visibility flag, toggled
/// by the gear and set by the `View > Appearance…` command.
pub fn show(
    ui: &mut egui::Ui,
    style: &mut AppStyle,
    screenshot: &Screenshot,
    appearance_open: &mut bool,
) -> (Option<MenuAction>, bool) {
    let mut action = None;
    let mut style_changed = false;
    let mut appearance_button = None;

    egui::MenuBar::new().ui(ui, |ui| {
        for menu in MENUS {
            ui.menu_button(menu.title, |ui| {
                for item in menu.items {
                    if add_menu_item(ui, item) {
                        action = Some(item.action);
                    }
                }
            });
        }

        // Ticket 59: the Settings menu held only Appearance…, so it is now a
        // direct gear toggle that anchors the same popover as View > Appearance….
        let gear = ui
            .button(APPEARANCE_BUTTON_LABEL)
            .on_hover_text("Appearance");
        if gear.clicked() {
            *appearance_open = !*appearance_open;
        }
        appearance_button = Some(gear);

        // The screenshot action lives at the right end of the bar and
        // dispatches the same ExportScreenshot action as File > Export.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if screenshot_button(ui, screenshot) {
                action = Some(MenuAction::ExportScreenshot);
            }
        });
    });

    if let Some(button) = appearance_button {
        style_changed = appearance_popover(ui, style, &button, appearance_open);
    }

    (action, style_changed)
}

/// Draw the compact (phone) bar: hamburger with every [`MENUS`] command, the
/// Appearance gear, and the Screenshot action. Same return contract as
/// [`show`], and the same actions — only the arrangement changes.
pub fn show_compact(
    ui: &mut egui::Ui,
    style: &mut AppStyle,
    screenshot: &Screenshot,
    appearance_open: &mut bool,
) -> (Option<MenuAction>, bool) {
    let mut action = None;
    let mut style_changed = false;
    let mut appearance_button = None;
    let touch = crate::ui::layout::TOUCH_TARGET;

    ui.spacing_mut().interact_size.y = touch;
    ui.horizontal(|ui| {
        // The hamburger is a menu button, so it needs the explicit 44 pt size
        // instead of the row's height alone.
        let hamburger =
            egui::Button::new(HAMBURGER_LABEL).min_size(egui::vec2(touch, touch));
        egui::menu::MenuButton::from_button(hamburger).ui(ui, |ui| {
            // One flat dropdown: nested submenus are awkward to hit with a
            // thumb, and every group title still labels its commands.
            for menu in MENUS {
                ui.label(egui::RichText::new(menu.title).strong());
                for item in menu.items {
                    if add_menu_item(ui, item) {
                        action = Some(item.action);
                    }
                }
                ui.separator();
            }
        });

        let gear = ui
            .add_sized(
                [touch, touch],
                egui::Button::new(APPEARANCE_BUTTON_LABEL),
            )
            .on_hover_text("Appearance");
        if gear.clicked() {
            *appearance_open = !*appearance_open;
        }
        appearance_button = Some(gear);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if screenshot_button(ui, screenshot) {
                action = Some(MenuAction::ExportScreenshot);
            }
        });
    });

    if let Some(button) = appearance_button {
        style_changed = appearance_popover(ui, style, &button, appearance_open);
    }

    (action, style_changed)
}

/// The About dialog: the 128 px mark, name, build version, tagline, copyright,
/// licence and both project links. `open` is the visibility flag; the window's
/// × clears it. Ticket 68 replaced the text-only body with this identity card.
pub fn about_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new(format!("About {}", menu::ABOUT_NAME))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    branding::logo_image(ui.ctx(), 128.0).alt_text("Vector Field Explorer logo"),
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(menu::ABOUT_NAME).size(22.0).strong());
                    ui.label(format!("Version {}", menu::ABOUT_VERSION));
                    ui.add_space(4.0);
                    ui.label(menu::ABOUT_TAGLINE);
                });
            });
            ui.separator();
            ui.label(menu::ABOUT_COPYRIGHT);
            ui.label(format!("License: {}", menu::ABOUT_LICENSE));
            ui.add_space(4.0);
            ui.hyperlink_to(menu::ABOUT_URL, menu::ABOUT_URL);
            ui.hyperlink_to(menu::ABOUT_SITE_URL, menu::ABOUT_SITE_URL);
        });
}
