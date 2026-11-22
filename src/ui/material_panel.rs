//! The dedicated Materials panel and its draft/commit seam.
//!
//! The panel edits one [`MaterialScene`] draft. Only a valid edit is handed to
//! [`State::set_material_scene`], so the committed scene never sees a rejected
//! value and the last valid arrows stay on screen. The draft survives frames
//! without a hidden dirty flag: a rejected edit keeps both its text and the
//! actionable error.
//!
//! Warnings shown in the panel always come from the sampling that belongs to
//! the committed scene. A valid scene edit clears the cached sampling, so a
//! warning from a prior scene can never be presented as current.

use eframe::egui;

use crate::material::{
    MATERIAL_COINCIDENT_SLICE_FACTOR, MATERIAL_EXACT_HIT_FACTOR, MaterialCase, MaterialFamily,
    MaterialScene, MaterialView, material_applied, material_assumptions, material_family,
    material_warning,
};
use crate::plane::PlaneAxis;
use crate::ui::help_overlay::HelpRow;
use crate::viewmodel::State;

/// The exact arrows-only note shared by the panel and the Features section.
pub const MATERIALS_ONLY_NOTE: &str =
    "Arrows only: streamlines, particles and click seeding stay off in Materials.";

/// A one-shot request from the panel to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialPanelAction {
    /// Frame the whole domain through the shared view-command seam.
    Fit,
    /// Restore the default material scene and the default camera.
    Reset,
}

/// Panel-local state: the draft scene, the last rejected edit and the Advanced
/// disclosure state.
#[derive(Debug, Clone, Default)]
pub struct MaterialPanelState {
    pub draft: MaterialScene,
    pub error: Option<String>,
    pub advanced: bool,
}

impl MaterialPanelState {
    /// Validate and hand the draft to the viewmodel. A rejected edit keeps the
    /// committed scene and the draft text, and records the actionable error.
    pub fn commit(&mut self, state: &mut State) -> bool {
        match state.set_material_scene(self.draft.clone()) {
            Ok(()) => {
                self.draft = state.material.clone();
                self.error = None;
                true
            }
            Err(_) => {
                self.error = state
                    .material_error
                    .clone()
                    .or_else(|| Some("Invalid material scene.".to_owned()));
                false
            }
        }
    }

    /// Refresh the draft from the committed scene. Called on mode entry, after
    /// Reset and after a persistence restore, never every frame, so a dirty
    /// draft is not overwritten while the user edits.
    pub fn sync_from(&mut self, state: &State) {
        self.draft = state.material.clone();
        self.error = None;
    }
}

/// Stable UI text for a case; the formula-bar summary and the viewport label use
/// the same names as the panel.
pub fn case_label(case: MaterialCase) -> &'static str {
    match case {
        MaterialCase::DielectricSphere => "Dielectric sphere",
        MaterialCase::DielectricBoundary => "Dielectric boundary",
        MaterialCase::PermeableSphere => "Permeable sphere",
        MaterialCase::PermeableBoundary => "Permeable boundary",
    }
}

/// Stable UI text for a view.
pub fn view_label(view: MaterialView) -> &'static str {
    match view {
        MaterialView::Total => "Total",
        MaterialView::Applied => "Applied",
        MaterialView::Response => "Response",
    }
}

/// `E` for electric cases, `B` for magnetic cases.
pub fn family_symbol(case: MaterialCase) -> &'static str {
    match material_family(case) {
        MaterialFamily::Electric => "E",
        MaterialFamily::Magnetic => "B",
    }
}

/// The exact display unit for the applied field family.
pub fn family_unit(case: MaterialCase) -> &'static str {
    match material_family(case) {
        MaterialFamily::Electric => "V/m",
        MaterialFamily::Magnetic => "T",
    }
}

/// Draw the Materials panel. `compact` applies the same finger-sized row floor
/// as the other compact sheets; wide keeps ordinary panel spacing. The caller
/// applies the returned action after the panel has finished painting.
pub fn show(
    ui: &mut egui::Ui,
    panel: &mut MaterialPanelState,
    state: &mut State,
    compact: bool,
) -> Option<MaterialPanelAction> {
    if compact {
        ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
        ui.spacing_mut().item_spacing.y = crate::ui::layout::COMPACT_ITEM_SPACING;
    }

    let mut commit_now = false;
    let mut action = None;

    ui.heading("Materials");

    // A rejected draft error stays actionable until the next successful commit
    // or Reset; it is never replaced by a warning from another scene.
    if let Some(error) = panel.error.as_ref().or(state.material_error.as_ref()) {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }

    ui.label("Case");
    ui.horizontal_wrapped(|ui| {
        for case in [
            MaterialCase::DielectricSphere,
            MaterialCase::DielectricBoundary,
            MaterialCase::PermeableSphere,
            MaterialCase::PermeableBoundary,
        ] {
            if ui
                .selectable_label(panel.draft.case == case, case_label(case))
                .clicked()
                && panel.draft.case != case
            {
                panel.draft.case = case;
                commit_now = true;
            }
        }
    });

    crate::ui::object_style_control::show(ui, state);

    ui.label("Geometry");
    match panel.draft.case {
        MaterialCase::DielectricSphere | MaterialCase::PermeableSphere => {
            let (_, release) = drag_edit(ui, &mut panel.draft.radius, "Radius", 0.0, 0.05);
            commit_now |= release;
        }
        MaterialCase::DielectricBoundary | MaterialCase::PermeableBoundary => {
            ui.horizontal(|ui| {
                ui.label("Interface axis");
                for axis in PlaneAxis::ALL {
                    let label = axis.label().to_uppercase();
                    if ui
                        .selectable_label(panel.draft.interface_axis == axis, label)
                        .clicked()
                        && panel.draft.interface_axis != axis
                    {
                        panel.draft.interface_axis = axis;
                        commit_now = true;
                    }
                }
            });
            let (_, release) =
                drag_edit(ui, &mut panel.draft.interface_offset, "Offset", 0.0, 0.05);
            commit_now |= release;
        }
    }

    ui.label("Background");
    ui.label("ε0 / μ0 are reference constants; these relative values belong to this Materials case only.");
    let (_, release) = drag_edit(
        ui,
        &mut panel.draft.background_epsilon_r,
        "Background εr",
        0.0,
        0.05,
    );
    commit_now |= release;
    let (_, release) = drag_edit(
        ui,
        &mut panel.draft.background_mu_r,
        "Background μr",
        0.0,
        0.05,
    );
    commit_now |= release;

    ui.label("Material");
    let family = material_family(panel.draft.case);
    let active = match family {
        MaterialFamily::Electric => "Active family: E (the material εr drives this case)",
        MaterialFamily::Magnetic => "Active family: B (the material μr drives this case)",
    };
    ui.label(active);
    let (_, release) = drag_edit(
        ui,
        &mut panel.draft.material_epsilon_r,
        "Material εr",
        0.0,
        0.05,
    );
    commit_now |= release;
    let (_, release) = drag_edit(ui, &mut panel.draft.material_mu_r, "Material μr", 0.0, 0.05);
    commit_now |= release;

    let symbol = family_symbol(panel.draft.case);
    let unit = family_unit(panel.draft.case);
    ui.label(format!("Applied {symbol} ({unit})"));
    for (index, axis) in ["x", "y", "z"].into_iter().enumerate() {
        let (_, release) = drag_edit(
            ui,
            &mut panel.draft.applied[index],
            axis,
            f64::NEG_INFINITY,
            0.05,
        );
        commit_now |= release;
    }

    ui.label("View");
    ui.horizontal_wrapped(|ui| {
        for view in [
            MaterialView::Total,
            MaterialView::Applied,
            MaterialView::Response,
        ] {
            if ui
                .selectable_label(panel.draft.view == view, view_label(view))
                .clicked()
                && panel.draft.view != view
            {
                panel.draft.view = view;
                commit_now = true;
            }
        }
    });

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Fit")
            .on_hover_text("Frame the durable domain or the selected plane (F)")
            .clicked()
        {
            action = Some(MaterialPanelAction::Fit);
        }
        if ui
            .button("Reset Materials section")
            .on_hover_text("Restore the default material scene and camera")
            .clicked()
        {
            panel.draft = MaterialScene::default();
            panel.error = None;
            action = Some(MaterialPanelAction::Reset);
        }
    });

    ui.label(MATERIALS_ONLY_NOTE);

    let mut advanced = panel.advanced;
    let response = egui::CollapsingHeader::new("Advanced")
        .open(Some(advanced))
        .show(ui, |ui| {
            show_advanced(ui, panel, state);
        });
    if response.header_response.clicked() {
        advanced = !advanced;
    }
    panel.advanced = advanced;

    if commit_now && action.is_none() {
        panel.commit(state);
    }
    action
}

fn drag_edit(
    ui: &mut egui::Ui,
    value: &mut f64,
    label: &str,
    minimum: f64,
    speed: f64,
) -> (bool, bool) {
    let response = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::DragValue::new(value)
                    .speed(speed)
                    .range(minimum..=f64::INFINITY),
            )
        })
        .inner;
    let changed = response.changed();
    // Ordinary drag release and a text edit that loses focus commit once; a
    // frame never commits merely because a value exists.
    let release = response.drag_stopped()
        || response.lost_focus()
        || (changed && ui.input(|input| input.key_pressed(egui::Key::Enter)));
    (changed, release)
}

fn show_advanced(ui: &mut egui::Ui, panel: &MaterialPanelState, state: &State) {
    ui.label("Assumptions");
    ui.label(material_assumptions(panel.draft.case));
    ui.label(format!(
        "Exact-hit omission: {:.0e} × max(extent, 1). Coincident-slice detection: {:.0e} × max(domain extent, case extent, 1).",
        MATERIAL_EXACT_HIT_FACTOR, MATERIAL_COINCIDENT_SLICE_FACTOR
    ));

    match state.material_sampling.as_ref() {
        Some(sampling) => {
            ui.label("Status: sampled");
            ui.label(format!(
                "Omitted interface samples: {}",
                sampling.omitted_interface
            ));
            if sampling.response_empty {
                ui.label(crate::material::MATERIAL_RESPONSE_EMPTY_WARNING);
            }
            if let Some(warning) = &sampling.warning {
                ui.colored_label(ui.visuals().warn_fg_color, warning);
            }
        }
        None => {
            ui.label("Status: no material results sampled yet");
        }
    }

    ui.label(material_warning(panel.draft.case));
    ui.label("No general material solver");
}

/// Materials-mode help rows, shared by the Help window and tests. They state the
/// bounded model's assumptions, the applied/response split, the exact units and
/// the interface rule.
pub static MATERIAL_HELP_ROWS: &[HelpRow] = &[
    HelpRow {
        input: "Model",
        action: "homogeneous, linear, isotropic, nondispersive, nonconducting, quasistatic dielectric or permeable cases; one inclusion in an infinite homogeneous background",
    },
    HelpRow {
        input: "Scope",
        action: "four bounded analytic cases; no general solver and no arbitrary geometry",
    },
    HelpRow {
        input: "Applied / response",
        action: "the applied field is uniform at infinity; response is total minus applied, never a second independent solve",
    },
    HelpRow {
        input: "Units",
        action: "applied E is drawn in V/m and applied B in T; no generic display-unit conversion",
    },
    HelpRow {
        input: "Interface",
        action: "the field can be discontinuous at an interface; samples exactly on it are omitted and the omission is reported",
    },
    HelpRow {
        input: "Arrows only",
        action: "streamlines, particles and click seeding stay off in Materials",
    },
];

/// The Materials help rows, exposed for the Help window and tests.
pub fn material_help_rows() -> &'static [HelpRow] {
    MATERIAL_HELP_ROWS
}

/// One-line summary used by the formula bar. Kept here so the selector and the
/// panel cannot disagree about the selected case or view.
pub fn summary(state: &State) -> String {
    let scene = &state.material;
    let family = family_symbol(scene.case);
    let applied = material_applied(scene);
    format!(
        "{} · {} · applied {family} ({}) · [{:.2}, {:.2}, {:.2}]",
        case_label(scene.case),
        view_label(scene.view),
        family_unit(scene.case),
        applied.x,
        applied.y,
        applied.z
    )
}
