//! The Appearance panel (issue #84 slice A2b).
//!
//! Edits the six [`LayerId`] records and the two [`PhysicsFamily`] records in
//! [`crate::layer_style::LayerStyles`]. Every control copies the committed
//! record, changes exactly one field and hands it to
//! [`State::commit_layer_styles`], which validates atomically: a rejected value
//! leaves both the styles and the dirty flag untouched. The panel is live/raster
//! styling only — the SVG/vector export keeps the unstyled blueprint.

use eframe::egui;

use crate::color_ramp::Rgb;
use crate::layer_style::{
    LayerId, LayerStyles, MAX_WIDTH_MULTIPLIER, MIN_WIDTH_MULTIPLIER, PhysicsFamily,
    ResolvedLayerStyle,
};
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

/// Panel-local state: the last rejected edit, if any.
#[derive(Debug, Clone, Default)]
pub struct AppearancePanelState {
    pub error: Option<String>,
}

impl AppearancePanelState {
    /// Apply one edit through the panel's own commit path, recording any
    /// validation error inline. The real controls call this for every change.
    pub fn apply(&mut self, state: &mut State, edit: AppearanceEdit) {
        self.error = apply_edit(state, edit).err();
    }

    /// Draw the panel body. `compact` raises the row height to the shared
    /// finger-sized floor used by the compact bottom sheet.
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, compact: bool) {
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

/// The trailing readout: an explicit value, or the effective fallback when the
/// field is unset.
fn fallback_label(current: Option<f32>, fallback: f32, decimals: usize) -> String {
    match current {
        Some(value) => format!("{value:.decimals$}"),
        None => format!("Default ({fallback:.decimals$})"),
    }
}
