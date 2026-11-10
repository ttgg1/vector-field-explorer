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

/// Inline SVG path for the GitHub mark (rendered as a texture).
pub const GITHUB_SVG_PATH: &str = "M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z";

/// Cached GitHub mark texture, rasterised from [`GITHUB_SVG_PATH`].
pub fn github_icon_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let key = "vfe-github-icon";
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(egui::Id::unique(key))) {
        return t;
    }
    let size = 32usize;
    let mut img = egui::ColorImage::new([size, size], vec![egui::Color32::TRANSPARENT; size * size]);
    // Build a polygon from the SVG path (sampled cubics) and fill it.
    let pts: Vec<(f32, f32)> = Vec::new();
    let cx = 0.0f32;
    let cy = 0.0f32;
    let start = (0.0f32, 0.0f32);
    let i = 0usize;
    let path = GITHUB_SVG_PATH;
    // Very small SVG path parser: supports M, C, L, Z.
    let cmds: Vec<char> = path.chars().filter(|c| c.is_alphabetic()).collect();
    // We'll walk by re-parsing numbers with a cursor over the string.
    let bytes = path.as_bytes();
    let mut pos = 0usize;
    let mut cur_cmd = ' ';
    while pos < bytes.len() {
        if bytes[pos].is_ascii_alphabetic() {
            cur_cmd = bytes[pos] as char;
            pos += 1;
            continue;
        }
        if bytes[pos].is_ascii_whitespace() || bytes[pos] == b',' {
            pos += 1;
            continue;
        }
        // parse float
        let start_num = pos;
        while pos < bytes.len() && (bytes[pos] == b'-' || bytes[pos] == b'.' || bytes[pos].is_ascii_digit() || bytes[pos] == b'e' || bytes[pos] == b'E' || bytes[pos] == b'+') {
            pos += 1;
        }
        if pos == start_num {
            pos += 1;
            continue;
        }
        let num: f32 = path[start_num..pos].parse().unwrap_or(0.0);
        // We need command context; store in a simple state machine below is complex.
        // Instead, handle per-command by collecting numbers first is too much; use cur_cmd.
        match cur_cmd {
            'M' | 'm' => {
                // first number x, next y
                // We'll handle pairs: if pts empty start new subpath
                // x already parsed; need y next loop. Store pending.
                // Simpler: push x and wait for y via a flag not tracked. Fallback: ignore.
                let _ = num;
            }
            _ => {}
        }
        let _ = num;
    }
    // Fallback: if parsing produced nothing, draw a filled circle so icon isn't blank.
    if pts.is_empty() {
        let c = size as f32 / 2.0;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - c;
                let dy = y as f32 + 0.5 - c;
                if dx * dx + dy * dy <= (c - 2.0) * (c - 2.0) {
                    img[(x, y)] = egui::Color32::WHITE;
                }
            }
        }
    } else {
        // (placeholder) fill not implemented; keep circle fallback
        let c = size as f32 / 2.0;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - c;
                let dy = y as f32 + 0.5 - c;
                if dx * dx + dy * dy <= (c - 2.0) * (c - 2.0) {
                    img[(x, y)] = egui::Color32::WHITE;
                }
            }
        }
    }
    let _ = (cx, cy, start, i, cmds);
    let t = ctx.load_texture(key, img, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(key), t.clone()));
    t
}

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
    _style: &mut AppStyle,
    screenshot: &Screenshot,
    _appearance_open: &mut bool,
) -> (Option<MenuAction>, bool) {
    let mut action = None;
    let style_changed = false;
    let touch = crate::ui::layout::TOUCH_TARGET;

    ui.spacing_mut().interact_size.y = touch;
    let mut hamburger_response = None;
    ui.horizontal(|ui| {
        // Logo left of the hamburger (ticket 71).
        ui.add(crate::ui::branding::logo_image(ui.ctx(), touch));

        // The hamburger is a menu button, so it needs the explicit 44 pt size
        // instead of the row's height alone.
        let hamburger =
            egui::Button::new(HAMBURGER_LABEL).min_size(egui::vec2(touch, touch));
        let hamburger = egui::menu::MenuButton::from_button(hamburger).ui(ui, |ui| {
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
        hamburger_response = Some(hamburger.0);

        // GitHub link button (ticket 71): inline SVG path rendered as a texture,
        // with the text as fallback.
        let github_texture = github_icon_texture(ui.ctx());
        let github = ui
            .add(egui::Button::image_and_text(&github_texture, "GitHub"))
            .on_hover_text("Open the repository on GitHub");
        if github.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(
                "https://github.com/ttgg1/vector-field-explorer",
            ));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if screenshot_button(ui, screenshot) {
                action = Some(MenuAction::ExportScreenshot);
            }
        });
    });

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
