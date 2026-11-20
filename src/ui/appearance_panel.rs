//! The Appearance panel (issue #84 slice A2b).
//!
//! Edits the six [`LayerId`] records and the two [`PhysicsFamily`] records in
//! [`crate::layer_style::LayerStyles`]. Every control copies the committed
//! record, changes exactly one field and hands it to
//! [`State::commit_layer_styles`], which validates atomically: a rejected value
//! leaves both the styles and the dirty flag untouched. The panel is live/raster
//! styling only — the SVG/vector export keeps the unstyled blueprint.

use eframe::egui;

use crate::appearance_profile::{AppearanceProfile, ProfileDocument, builtins};
use crate::color_ramp::Rgb;
use crate::layer_style::{
    LayerId, LayerStyles, MAX_WIDTH_MULTIPLIER, MIN_WIDTH_MULTIPLIER, PhysicsFamily,
    ResolvedLayerStyle,
};
use crate::profile_library::ProfileLibrary;
use crate::viewmodel::State;

/// One field edit the panel can request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppearanceEdit {
    /// Set or clear one layer's opacity multiplier.
    LayerOpacity(LayerId, Option<f32>),
    /// Set or clear one layer's width multiplier.
    LayerWidth(LayerId, Option<f32>),
    /// Set or clear one family's opacity multiplier.
    FamilyOpacity(PhysicsFamily, Option<f32>),
    /// Set or clear one family's width multiplier.
    FamilyWidth(PhysicsFamily, Option<f32>),
    /// Set or clear one family's fixed-colour override.
    FamilyColor(PhysicsFamily, Option<Rgb>),
}

/// The index of `layer` in [`LayerId::ALL`] and [`LayerStyles::layers`].
fn layer_index(layer: LayerId) -> usize {
    LayerId::ALL
        .iter()
        .position(|candidate| *candidate == layer)
        .expect("every layer is listed in LayerId::ALL")
}

/// The index of `family` in [`PhysicsFamily::ALL`] and [`LayerStyles::families`].
fn family_index(family: PhysicsFamily) -> usize {
    PhysicsFamily::ALL
        .iter()
        .position(|candidate| *candidate == family)
        .expect("every family is listed in PhysicsFamily::ALL")
}

/// The effective family fallback: global baseline plus that family only. Layer
/// overrides are deliberately excluded, so the family row shows and seeds what
/// an unset layer field would fall back to. [`LayerStyles::resolve`] still
/// applies the layer over the family for the actual drawable precedence.
pub fn resolve_family(styles: &LayerStyles, family: PhysicsFamily) -> ResolvedLayerStyle {
    let overrides = styles.families[family_index(family)];
    ResolvedLayerStyle {
        opacity: overrides.opacity.unwrap_or(1.0),
        width: overrides.width.unwrap_or(1.0),
        color: overrides.color,
    }
}

/// Apply one edit through [`State::commit_layer_styles`]. The whole record is
/// validated, so an invalid direct write is rejected with a message and leaves
/// the committed styles untouched.
pub fn apply_edit(state: &mut State, edit: AppearanceEdit) -> Result<(), String> {
    let mut styles = *state.layer_styles();
    match edit {
        AppearanceEdit::LayerOpacity(layer, value) => {
            styles.layers[layer_index(layer)].opacity = value;
        }
        AppearanceEdit::LayerWidth(layer, value) => {
            styles.layers[layer_index(layer)].width = value;
        }
        AppearanceEdit::FamilyOpacity(family, value) => {
            styles.families[family_index(family)].opacity = value;
        }
        AppearanceEdit::FamilyWidth(family, value) => {
            styles.families[family_index(family)].width = value;
        }
        AppearanceEdit::FamilyColor(family, value) => {
            styles.families[family_index(family)].color = value;
        }
    }
    state.commit_layer_styles(styles)
}

/// Restore every layer and family override to neutral through the same
/// validating commit seam.
pub fn reset_all(state: &mut State) -> Result<(), String> {
    state.commit_layer_styles(LayerStyles::default())
}

/// Panel-local state: the last rejected edit or profile action, the success
/// message, and the session-only profile drafts.
#[derive(Debug, Clone, Default)]
pub struct AppearancePanelState {
    pub error: Option<String>,
    /// The last successful profile action, painted in a weak colour.
    pub message: Option<String>,
    /// The save-current name draft.
    pub profile_name: String,
    /// The import draft.
    pub import_text: String,
    /// The cached portable export, recomputed after every user-library mutation
    /// and when the user clicks Export.
    pub export_text: String,
    /// The name awaiting the second delete click.
    pub pending_delete: Option<String>,
}

impl AppearancePanelState {
    /// Apply one edit through the panel's own commit path, recording any
    /// validation error inline. The real controls call this for every change.
    pub fn apply(&mut self, state: &mut State, edit: AppearanceEdit) {
        match apply_edit(state, edit) {
            Ok(()) => self.error = None,
            Err(error) => self.fail(error),
        }
    }

    /// Apply one profile through [`AppearanceProfile::apply`], recording a
    /// success message or the rejection inline.
    pub fn apply_profile(&mut self, state: &mut State, profile: &AppearanceProfile) {
        match profile.apply(state) {
            Ok(()) => self.succeed(format!("Applied {}", profile.name)),
            Err(error) => self.fail(error),
        }
    }

    /// Draw the panel body. `compact` raises the row height to the shared
    /// finger-sized floor used by the compact bottom sheet.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut State,
        profiles: &mut ProfileLibrary,
        compact: bool,
    ) {
        if compact {
            ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
            ui.spacing_mut().item_spacing.y = crate::ui::layout::COMPACT_ITEM_SPACING;
        }

        ui.heading("Appearance");
        ui.label(
            "Styling affects the live and raster-rendered view only. \
             SVG/vector exports keep the unstyled blueprint.",
        );
        if let Some(error) = self.error.as_ref() {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(message) = self.message.as_ref() {
            ui.weak(message);
        }

        self.profile_section(ui, state, profiles);

        ui.separator();
        ui.label(egui::RichText::new("Layers").strong());
        for layer in LayerId::ALL {
            self.layer_row(ui, state, layer);
        }

        ui.separator();
        ui.label(egui::RichText::new("Physics families").strong());
        for family in PhysicsFamily::ALL {
            self.family_row(ui, state, family);
        }

        ui.separator();
        if ui
            .button("Reset all")
            .on_hover_text("Restore every layer and family override to neutral")
            .clicked()
        {
            self.error = reset_all(state).err();
        }
    }

    fn profile_section(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut State,
        profiles: &mut ProfileLibrary,
    ) {
        ui.separator();
        ui.label(egui::RichText::new("Profiles").strong());

        let shipped = builtins();
        let builtin_count = shipped.len();
        let all = profiles.all();
        for (index, profile) in all.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(&profile.name);
                if ui
                    .button("Apply")
                    .on_hover_text("Apply this appearance profile")
                    .clicked()
                {
                    self.apply_profile(state, profile);
                    if self.error.is_none() {
                        self.pending_delete = None;
                    }
                }
                if index >= builtin_count {
                    let label = if self.pending_delete.as_deref() == Some(profile.name.as_str()) {
                        "Confirm delete"
                    } else {
                        "Delete"
                    };
                    if ui
                        .button(label)
                        .on_hover_text("Delete this user profile")
                        .clicked()
                    {
                        self.delete_profile(profiles, &profile.name);
                    }
                }
            });
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            ui.add(
                egui::TextEdit::singleline(&mut self.profile_name)
                    .hint_text("Profile name")
                    .desired_width(180.0),
            );
        });
        if ui.button("Save current appearance").clicked() {
            self.save_current(state, profiles);
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button("Export text").clicked() {
                self.sync_export(profiles);
            }
            let can_copy = !self.export_text.is_empty();
            if ui
                .add_enabled(can_copy, egui::Button::new("Copy"))
                .on_hover_text("Copy the profile document")
                .clicked()
            {
                let text = self.export_text.clone();
                ui.ctx().copy_text(text);
                self.succeed("Copied profile text".to_owned());
            }
        });
        ui.add(
            egui::TextEdit::multiline(&mut self.export_text)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .interactive(false),
        );

        ui.label("Import profiles");
        ui.add(
            egui::TextEdit::multiline(&mut self.import_text)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Paste a profile document"),
        );
        if ui.button("Apply text").clicked() {
            self.import_profiles(profiles);
        }
    }

    fn save_current(&mut self, state: &State, profiles: &mut ProfileLibrary) {
        let name = self.profile_name.trim().to_owned();
        if name.is_empty() {
            self.fail("enter a profile name before saving".to_owned());
            return;
        }
        if builtins().iter().any(|builtin| builtin.name == name) {
            self.fail(format!(
                "profile name {name:?} is built in and cannot be overwritten"
            ));
            return;
        }

        let replaces = profiles.user().iter().any(|profile| profile.name == name);
        let profile = AppearanceProfile::capture(name.clone(), state);
        match profiles.upsert(profile) {
            Ok(()) => {
                self.profile_name.clear();
                self.pending_delete = None;
                self.sync_export(profiles);
                let action = if replaces { "Replaced" } else { "Saved" };
                self.succeed(format!("{action} profile {name}"));
            }
            Err(error) => self.fail(error),
        }
    }

    fn delete_profile(&mut self, profiles: &mut ProfileLibrary, name: &str) {
        if self.pending_delete.as_deref() != Some(name) {
            self.pending_delete = Some(name.to_owned());
            self.succeed(format!("Click Delete again to remove {name}"));
            return;
        }

        self.pending_delete = None;
        if profiles.remove(name) {
            self.sync_export(profiles);
            self.succeed(format!("Deleted profile {name}"));
        } else {
            self.fail(format!("profile {name} was not found"));
        }
    }

    fn import_profiles(&mut self, profiles: &mut ProfileLibrary) {
        let before = profiles.len();
        match profiles.import_text(&self.import_text) {
            Ok(after) => {
                self.sync_export(profiles);
                let added = after.saturating_sub(before);
                let message = match added {
                    0 if ProfileDocument::from_json(&self.import_text)
                        .is_ok_and(|document| document.profiles.is_empty()) =>
                    {
                        "Nothing to import".to_owned()
                    }
                    0 => "Import replaced existing user profiles; no new profiles imported"
                        .to_owned(),
                    1 => "Imported 1 new user profile".to_owned(),
                    count => format!("Imported {count} new user profiles"),
                };
                self.succeed(message);
            }
            Err(error) => self.fail(error),
        }
    }

    /// Recompute the cached portable export. Every user-library mutation and the
    /// Export click call this one helper, so no revision bookkeeping is needed.
    fn sync_export(&mut self, profiles: &ProfileLibrary) {
        self.export_text = export_buffer(profiles);
    }

    fn succeed(&mut self, message: String) {
        self.error = None;
        self.message = Some(message);
    }

    fn fail(&mut self, error: String) {
        self.error = Some(error);
        self.message = None;
    }

    fn layer_row(&mut self, ui: &mut egui::Ui, state: &mut State, layer: LayerId) {
        let slot = layer_index(layer);
        let overrides = state.layer_styles().layers[slot];
        let resolved = state.layer_styles().resolve(layer, None);

        ui.group(|ui| {
            ui.label(egui::RichText::new(layer.label()).strong());
            let mut opacity = overrides.opacity.unwrap_or(resolved.opacity);
            ui.horizontal_wrapped(|ui| {
                ui.label("Opacity");
                if ui
                    .add(egui::Slider::new(&mut opacity, 0.0..=1.0).max_decimals(2))
                    .changed()
                {
                    self.apply(state, AppearanceEdit::LayerOpacity(layer, Some(opacity)));
                }
                if ui
                    .small_button("Default")
                    .on_hover_text("Use the global/family fallback")
                    .clicked()
                {
                    self.apply(state, AppearanceEdit::LayerOpacity(layer, None));
                }
                ui.weak(fallback_label(overrides.opacity, resolved.opacity, 2));
            });

            let mut width = overrides.width.unwrap_or(resolved.width);
            ui.horizontal_wrapped(|ui| {
                ui.label("Width");
                if ui
                    .add(
                        egui::Slider::new(&mut width, MIN_WIDTH_MULTIPLIER..=MAX_WIDTH_MULTIPLIER)
                            .max_decimals(2),
                    )
                    .changed()
                {
                    self.apply(state, AppearanceEdit::LayerWidth(layer, Some(width)));
                }
                if ui
                    .small_button("Default")
                    .on_hover_text("Use the global/family fallback")
                    .clicked()
                {
                    self.apply(state, AppearanceEdit::LayerWidth(layer, None));
                }
                ui.weak(fallback_label(overrides.width, resolved.width, 2));
            });
        });
    }

    fn family_row(&mut self, ui: &mut egui::Ui, state: &mut State, family: PhysicsFamily) {
        let slot = family_index(family);
        let overrides = state.layer_styles().families[slot];
        let resolved = resolve_family(state.layer_styles(), family);

        ui.group(|ui| {
            ui.label(egui::RichText::new(family.label()).strong());
            let mut opacity = overrides.opacity.unwrap_or(resolved.opacity);
            ui.horizontal_wrapped(|ui| {
                ui.label("Opacity");
                if ui
                    .add(egui::Slider::new(&mut opacity, 0.0..=1.0).max_decimals(2))
                    .changed()
                {
                    self.apply(state, AppearanceEdit::FamilyOpacity(family, Some(opacity)));
                }
                if ui
                    .small_button("Default")
                    .on_hover_text("Use the global fallback")
                    .clicked()
                {
                    self.apply(state, AppearanceEdit::FamilyOpacity(family, None));
                }
                ui.weak(fallback_label(overrides.opacity, resolved.opacity, 2));
            });

            let mut width = overrides.width.unwrap_or(resolved.width);
            ui.horizontal_wrapped(|ui| {
                ui.label("Width");
                if ui
                    .add(
                        egui::Slider::new(&mut width, MIN_WIDTH_MULTIPLIER..=MAX_WIDTH_MULTIPLIER)
                            .max_decimals(2),
                    )
                    .changed()
                {
                    self.apply(state, AppearanceEdit::FamilyWidth(family, Some(width)));
                }
                if ui
                    .small_button("Default")
                    .on_hover_text("Use the global fallback")
                    .clicked()
                {
                    self.apply(state, AppearanceEdit::FamilyWidth(family, None));
                }
                ui.weak(fallback_label(overrides.width, resolved.width, 2));
            });

            let seed = overrides.color.unwrap_or(Rgb([128, 128, 128]));
            let mut srgba = egui::Color32::from_rgb(seed.0[0], seed.0[1], seed.0[2]);
            ui.horizontal_wrapped(|ui| {
                ui.label("Colour");
                if ui.color_edit_button_srgba(&mut srgba).changed() {
                    let [r, g, b, _] = srgba.to_array();
                    let picked = Rgb([r, g, b]);
                    // An alpha-only change must not turn the seed grey into an
                    // explicit fixed-colour override.
                    if picked != seed || overrides.color.is_some() {
                        self.apply(state, AppearanceEdit::FamilyColor(family, Some(picked)));
                    }
                }
                if ui
                    .small_button("Default")
                    .on_hover_text("Keep the drawable's fixed or mapped colour")
                    .clicked()
                {
                    self.apply(state, AppearanceEdit::FamilyColor(family, None));
                }
                let effective = match overrides.color {
                    Some(color) => format!(
                        "#{:02x}{:02x}{:02x}",
                        color.0[0], color.0[1], color.0[2]
                    ),
                    None => "Default (drawable colour)".to_owned(),
                };
                ui.weak(effective);
            });
        });
    }
}

/// The portable buffer for a library: an empty user library exports as an empty
/// buffer so Copy stays disabled, otherwise the compact document text.
fn export_buffer(profiles: &ProfileLibrary) -> String {
    if profiles.is_empty() {
        String::new()
    } else {
        profiles.export_text()
    }
}

/// The trailing readout: an explicit value, or the effective fallback when the
/// field is unset.
fn fallback_label(current: Option<f32>, fallback: f32, decimals: usize) -> String {
    match current {
        Some(value) => format!("{value:.decimals$}"),
        None => format!("Default ({fallback:.decimals$})"),
    }
}
