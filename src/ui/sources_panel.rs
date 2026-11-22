//! The dedicated Sources panel and its draft/commit seam.
//!
//! The panel edits a list of [`SourceDraft`]s. A draft keeps the user's text
//! even when it is invalid; only a valid edit is handed to
//! [`State::set_source_scene`], so the committed scene and the saved payload
//! never see a rejected value. The viewport status is derived from the cached
//! [`SourceSampling`] rather than inventing a second accuracy accounting.

use eframe::egui;
use glam::DVec3;

use crate::custom_density::{
    ChargeReport, ContinuityReport, ContinuityStatus, DensityFamily, DensitySpec, DensitySupport,
    DiagnosticsStatus, MAX_SAMPLES, MIN_SAMPLES, SupportKind, TimeLaw,
};
use crate::object_scene::ObjectStyle;
use crate::source_scene::{
    FamilyColors, MAX_SOFTENING, MIN_SOFTENING, SourceDisplay, SourceEntry, SourceFamilyStatus,
    SourceKindTag, SourceSampling, SourceScene, error_message, scene_error_message,
    validate_source,
};
use crate::sources::{MAX_SOURCES, Source, SourceKind};
use crate::ui::object_style_control::OBJECT_STYLE_HELP;
use crate::viewmodel::{DensityReportRow, MAX_DENSITIES, State};

/// One editable source row. `committed` is always the last valid source; the
/// numeric mirrors may be invalid or non-finite while the user types.
#[derive(Debug, Clone)]
pub struct SourceDraft {
    pub committed: Source,
    pub enabled: bool,
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub strength: f64,
    pub radius: f64,
    pub length: f64,
    pub turns: u32,
    pub error: Option<String>,
    pub dirty: bool,
}

impl SourceDraft {
    fn from_entry(entry: &SourceEntry) -> Self {
        let source = entry.source;
        let (strength, radius, length, turns) = match source.kind {
            SourceKind::PointCharge { charge } => (charge, 0.0, 0.0, 0),
            SourceKind::FiniteLineCharge { charge, length } => (charge, 0.0, length, 0),
            SourceKind::ChargedRing { charge, radius } => (charge, radius, 0.0, 0),
            SourceKind::ChargedDisc { charge, radius } => (charge, radius, 0.0, 0),
            SourceKind::SolidSphere { charge, radius } => (charge, radius, 0.0, 0),
            SourceKind::StraightWire { current } => (current, 0.0, 0.0, 0),
            SourceKind::CurrentLoop { current, radius } => (current, radius, 0.0, 0),
            SourceKind::Solenoid {
                current,
                radius,
                length,
                turns,
            } => (current, radius, length, turns),
        };
        Self {
            committed: source,
            enabled: entry.enabled,
            center: source.center.to_array(),
            axis: source.axis.to_array(),
            strength,
            radius,
            length,
            turns,
            error: None,
            dirty: false,
        }
    }

    /// Rebuild the source from the draft's current text. Invalid values are
    /// still representable; the caller validates before committing.
    pub fn source(&self) -> Source {
        let kind = match self.committed.kind {
            SourceKind::PointCharge { .. } => SourceKind::PointCharge {
                charge: self.strength,
            },
            SourceKind::FiniteLineCharge { .. } => SourceKind::FiniteLineCharge {
                charge: self.strength,
                length: self.length,
            },
            SourceKind::ChargedRing { .. } => SourceKind::ChargedRing {
                charge: self.strength,
                radius: self.radius,
            },
            SourceKind::ChargedDisc { .. } => SourceKind::ChargedDisc {
                charge: self.strength,
                radius: self.radius,
            },
            SourceKind::SolidSphere { .. } => SourceKind::SolidSphere {
                charge: self.strength,
                radius: self.radius,
            },
            SourceKind::StraightWire { .. } => SourceKind::StraightWire {
                current: self.strength,
            },
            SourceKind::CurrentLoop { .. } => SourceKind::CurrentLoop {
                current: self.strength,
                radius: self.radius,
            },
            SourceKind::Solenoid { .. } => SourceKind::Solenoid {
                current: self.strength,
                radius: self.radius,
                length: self.length,
                turns: self.turns,
            },
        };
        Source {
            center: DVec3::from_array(self.center),
            axis: DVec3::from_array(self.axis),
            kind,
        }
    }

    fn tag(&self) -> SourceKindTag {
        SourceKindTag::of(&self.committed.kind)
    }
}

/// The editor's two-valued view of [`TimeLaw`]. Keeping the kind separate from
/// the numeric mirrors lets the widgets reveal omega and phase without
/// rebuilding the model enum. The mirrors live for the whole session: a
/// committed steady law carries no parameters, so `sync_densities` preserves
/// the row's omega/phase instead of re-seeding them, and re-selecting Harmonic
/// restores the user's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeLawKind {
    #[default]
    Steady,
    Harmonic,
}

impl TimeLawKind {
    fn label(self) -> &'static str {
        match self {
            Self::Steady => "Steady",
            Self::Harmonic => "Harmonic",
        }
    }

    fn is_harmonic(self) -> bool {
        matches!(self, Self::Harmonic)
    }
}

/// The omega a *fresh* steady row starts with. A row that has committed a
/// steady law keeps its own omega/phase mirrors instead of using this value.
const DEFAULT_OMEGA: f64 = 1.0;

/// One editable custom-density row. `committed` is the last valid spec; the
/// text and numeric mirrors stay session-only and may be invalid while typed.
#[derive(Debug, Clone)]
pub struct DensityDraft {
    pub committed: DensitySpec,
    pub name: String,
    pub family: DensityFamily,
    pub support_kind: SupportKind,
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub size: [f64; 3],
    pub formula: String,
    pub samples: u32,
    pub softening: f64,
    /// Mirror of the declared clock kind. The numeric mirrors below persist
    /// while the kind is steady so toggling back does not lose them.
    pub law_kind: TimeLawKind,
    pub omega: f64,
    pub phase: f64,
    /// Mirror of the optional paired charge density for a steady current.
    /// Kept verbatim while the law is steady even though the model ignores it.
    pub charge_formula: String,
    pub error: Option<String>,
    pub dirty: bool,
}

impl DensityDraft {
    fn from_spec(spec: &DensitySpec) -> Self {
        let (law_kind, omega, phase) = match spec.time_law {
            TimeLaw::Steady => (TimeLawKind::Steady, DEFAULT_OMEGA, 0.0),
            TimeLaw::Harmonic { omega, phase } => (TimeLawKind::Harmonic, omega, phase),
        };
        Self {
            committed: spec.clone(),
            name: spec.name.clone(),
            family: spec.family,
            support_kind: spec.support.kind,
            center: spec.support.center.as_dvec3().to_array(),
            axis: spec.support.axis.as_dvec3().to_array(),
            size: spec.support.size.as_dvec3().to_array(),
            formula: spec.formula.clone(),
            samples: spec.samples,
            softening: spec.softening,
            law_kind,
            omega,
            phase,
            charge_formula: spec.charge_formula.clone().unwrap_or_default(),
            error: None,
            dirty: false,
        }
    }

    /// Rebuild the spec from the mirrors. The caller validates before commit.
    pub fn spec(&self) -> DensitySpec {
        DensitySpec {
            name: self.name.clone(),
            family: self.family,
            support: DensitySupport {
                kind: self.support_kind,
                center: glam::Vec3::new(
                    self.center[0] as f32,
                    self.center[1] as f32,
                    self.center[2] as f32,
                ),
                axis: glam::Vec3::new(
                    self.axis[0] as f32,
                    self.axis[1] as f32,
                    self.axis[2] as f32,
                ),
                size: glam::Vec3::new(
                    self.size[0] as f32,
                    self.size[1] as f32,
                    self.size[2] as f32,
                ),
            },
            formula: self.formula.clone(),
            samples: self.samples,
            softening: self.softening,
            time_law: match self.law_kind {
                TimeLawKind::Steady => TimeLaw::Steady,
                TimeLawKind::Harmonic => TimeLaw::Harmonic {
                    omega: self.omega,
                    phase: self.phase,
                },
            },
            // Only a steady current may carry a pairing; a charge family's own
            // formula is its rho, so the hidden mirror must not leak into it.
            charge_formula: (self.family == DensityFamily::SteadyCurrent
                && !self.charge_formula.trim().is_empty())
            .then(|| self.charge_formula.clone()),
        }
    }
}

impl DensityFamily {
    fn label(self) -> &'static str {
        match self {
            DensityFamily::VolumeCharge => "Volume charge",
            DensityFamily::SurfaceCharge => "Surface charge",
            DensityFamily::SteadyCurrent => "Steady current",
        }
    }

    fn formula_hint(self) -> &'static str {
        match self {
            DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge => "scalar rho(x, y, z, t)",
            DensityFamily::SteadyCurrent => "Jx, Jy, Jz",
        }
    }
}

impl SupportKind {
    fn label(self) -> &'static str {
        match self {
            SupportKind::Sphere => "Sphere",
            SupportKind::Box => "Box",
            SupportKind::Cylinder => "Cylinder",
        }
    }
}

/// Panel-local state: the kind the Add control will create and one draft per
/// committed scene entry.
#[derive(Debug)]
pub struct SourcesPanelState {
    pub add_kind: SourceKindTag,
    pub drafts: Vec<SourceDraft>,
    /// Session-only custom-density editors (issue #96 slice B1); the committed
    /// list lives in `State::densities`.
    pub density_drafts: Vec<DensityDraft>,
    pub density_error: Option<String>,
    /// Rows with a continuous (drag/slider) density edit in flight, in the
    /// order they first changed. Each edit is committed once when the gesture
    /// ends, so a drag does not validate and re-sample the lattice on every
    /// frame. A bounded queue rather than one slot keeps a second row's text
    /// edit in the same frame from displacing an unfinished drag.
    pub density_pending_commits: Vec<usize>,
}

impl Default for SourcesPanelState {
    fn default() -> Self {
        Self {
            add_kind: SourceKindTag::PointCharge,
            drafts: Vec::new(),
            density_drafts: Vec::new(),
            density_error: None,
            density_pending_commits: Vec::new(),
        }
    }
}

impl SourcesPanelState {
    /// Rebuild drafts when the entry list changes structurally. On an
    /// add/remove the list positions no longer identify the same source, so
    /// preserving dirty state by index could commit one entry's text onto
    /// another; drop it and re-read the committed scene. With a stable length,
    /// only non-dirty drafts are refreshed so invalid text survives.
    pub fn sync(&mut self, scene: &SourceScene) {
        if self.drafts.len() != scene.entries.len() {
            self.drafts = scene.entries.iter().map(SourceDraft::from_entry).collect();
            return;
        }
        for (draft, entry) in self.drafts.iter_mut().zip(&scene.entries) {
            if !draft.dirty {
                *draft = SourceDraft::from_entry(entry);
            }
        }
    }

    /// Follow the committed density list. A structural change (add/remove)
    /// rebuilds every draft; otherwise only non-dirty drafts refresh so invalid
    /// text survives while the user types.
    pub fn sync_densities(&mut self, state: &State) {
        let committed = state.densities();
        if self.density_drafts.len() != committed.len() {
            self.density_drafts = committed.iter().map(DensityDraft::from_spec).collect();
            // Structural changes renumber the rows, so a stale index could
            // commit one row's mirror onto another. `remove_density` also
            // cancels the removed row's deferred edit before this runs.
            self.density_pending_commits.clear();
            return;
        }
        for (draft, spec) in self.density_drafts.iter_mut().zip(committed) {
            if !draft.dirty {
                // A steady spec carries no omega/phase, so preserve this row's
                // mirrors rather than resetting a value the user just hid by
                // selecting Steady. Only a truly harmonic spec re-seeds them.
                let harmonic = matches!(spec.time_law, TimeLaw::Harmonic { .. });
                let (omega, phase) = (draft.omega, draft.phase);
                *draft = DensityDraft::from_spec(spec);
                if !harmonic {
                    draft.omega = omega;
                    draft.phase = phase;
                }
            }
        }
    }

    /// Remember a row with an in-flight continuous edit, once. The queue is
    /// bounded by the document cap so it can never grow without limit.
    fn queue_pending(&mut self, index: usize) {
        if self.density_pending_commits.len() >= MAX_DENSITIES
            || self.density_pending_commits.contains(&index)
        {
            return;
        }
        self.density_pending_commits.push(index);
    }

    /// Take the deferred edits in first-changed order, leaving the queue empty.
    fn take_pending(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.density_pending_commits)
    }

    /// Drop one row's deferred edit; a text/combo edit on that row already
    /// commits the whole draft now.
    fn forget_pending(&mut self, index: usize) {
        self.density_pending_commits
            .retain(|&pending| pending != index);
    }

    /// Add a valid default density unless the document cap is reached. A real
    /// add commits through the model seam and refreshes reports once.
    pub fn add_density(&mut self, state: &mut State) -> Option<String> {
        if state.densities().len() >= MAX_DENSITIES {
            let message =
                format!("At most {MAX_DENSITIES} custom densities are allowed; remove one first.");
            self.density_error = Some(message.clone());
            return Some(message);
        }
        let used: Vec<&str> = state
            .densities()
            .iter()
            .map(|spec| spec.name.as_str())
            .collect();
        let mut serial = state.densities().len() + 1;
        let name = loop {
            let candidate = format!("Density {serial}");
            if !used.contains(&candidate.as_str()) {
                break candidate;
            }
            serial += 1;
        };
        let mut next = state.densities().to_vec();
        next.push(default_density(name));
        match state.commit_densities(next) {
            Ok(()) => {
                self.density_error = None;
                self.sync_densities(state);
                state.refresh_density_reports();
                None
            }
            Err(message) => {
                self.density_error = Some(message.clone());
                Some(message)
            }
        }
    }

    /// Remove one committed density. An out-of-range index is a no-op.
    pub fn remove_density(&mut self, index: usize, state: &mut State) -> Option<String> {
        if index >= state.densities().len() {
            return Some("That custom density no longer exists.".to_owned());
        }
        // Structural rebuild drops every draft mirror, so commit the other
        // rows' deferred continuous edits first. Only the removed row's own
        // pending index is cancelled.
        let pending = std::mem::take(&mut self.density_pending_commits);
        for pending_index in pending.into_iter().filter(|&pending| pending != index) {
            self.commit_density(pending_index, state);
        }
        let mut next = state.densities().to_vec();
        next.remove(index);
        match state.commit_densities(next) {
            Ok(()) => {
                self.density_error = None;
                self.sync_densities(state);
                state.refresh_density_reports();
                None
            }
            Err(message) => {
                self.density_error = Some(message.clone());
                Some(message)
            }
        }
    }

    /// Hand one draft to the atomic model seam. A rejected edit keeps the draft
    /// and the committed list; an accepted one is followed by one on-demand
    /// report refresh.
    pub fn commit_density(&mut self, index: usize, state: &mut State) -> Option<String> {
        let Some(draft) = self.density_drafts.get(index) else {
            return Some("That custom density no longer exists.".to_owned());
        };
        let spec = draft.spec();
        let mut next = state.densities().to_vec();
        let Some(slot) = next.get_mut(index) else {
            return Some("That custom density no longer exists.".to_owned());
        };
        *slot = spec;
        match state.commit_densities(next) {
            Ok(()) => {
                let committed = state.densities().get(index).cloned();
                if let (Some(draft), Some(committed)) =
                    (self.density_drafts.get_mut(index), committed)
                {
                    draft.committed = committed;
                    draft.error = None;
                    draft.dirty = false;
                }
                // A per-draft edit never owns the panel-level message: leaving
                // a cap/add/remove notice in place until its own condition
                // clears is the point of that banner.
                self.sync_densities(state);
                state.refresh_density_reports();
                None
            }
            Err(message) => {
                if let Some(draft) = self.density_drafts.get_mut(index) {
                    draft.dirty = true;
                    draft.error = Some(message.clone());
                }
                // The rejection is shown on this draft; a panel-level message
                // set by another action must survive it.
                Some(message)
            }
        }
    }

    /// Validate the draft at `index`, setting its error and dirty flag. Returns
    /// the readable error when invalid.
    pub fn mark_edited(&mut self, index: usize) -> Option<String> {
        let Some(draft) = self.drafts.get_mut(index) else {
            return Some("That source no longer exists.".to_owned());
        };
        let source = draft.source();
        match validate_source(&source) {
            Ok(()) => {
                draft.dirty = true;
                draft.error = None;
                None
            }
            Err(error) => {
                let message = format!("Source {}: {}", index + 1, error_message(&error));
                draft.dirty = true;
                draft.error = Some(message.clone());
                Some(message)
            }
        }
    }

    pub fn set_radius(&mut self, index: usize, value: f64) -> Option<String> {
        if let Some(draft) = self.drafts.get_mut(index) {
            draft.radius = value;
        }
        self.mark_edited(index)
    }

    pub fn set_strength(&mut self, index: usize, value: f64) -> Option<String> {
        if let Some(draft) = self.drafts.get_mut(index) {
            draft.strength = value;
        }
        self.mark_edited(index)
    }

    pub fn set_length(&mut self, index: usize, value: f64) -> Option<String> {
        if let Some(draft) = self.drafts.get_mut(index) {
            draft.length = value;
        }
        self.mark_edited(index)
    }

    pub fn set_turns(&mut self, index: usize, value: u32) -> Option<String> {
        if let Some(draft) = self.drafts.get_mut(index) {
            draft.turns = value;
        }
        self.mark_edited(index)
    }

    pub fn set_center(&mut self, index: usize, center: [f64; 3]) -> Option<String> {
        if let Some(draft) = self.drafts.get_mut(index) {
            draft.center = center;
        }
        self.mark_edited(index)
    }

    pub fn set_axis(&mut self, index: usize, axis: [f64; 3]) -> Option<String> {
        if let Some(draft) = self.drafts.get_mut(index) {
            draft.axis = axis;
        }
        self.mark_edited(index)
    }

    /// Hand the draft's source and enable state to the committed scene. Invalid
    /// input leaves the scene untouched and keeps the draft's visible error.
    pub fn commit(&mut self, index: usize, state: &mut State) -> Option<String> {
        let Some(draft) = self.drafts.get(index) else {
            return Some("That source no longer exists.".to_owned());
        };
        let source = draft.source();
        let enabled = draft.enabled;
        let error = match validate_source(&source) {
            Ok(()) => None,
            Err(error) => Some(format!("Source {}: {}", index + 1, error_message(&error))),
        };
        if let Some(message) = error {
            if let Some(draft) = self.drafts.get_mut(index) {
                draft.dirty = true;
                draft.error = Some(message.clone());
            }
            return Some(message);
        }

        let mut scene = state.sources.clone();
        scene.set_enabled(index, enabled);
        if let Err(error) = scene.replace(index, source) {
            let message = scene_error_message(&error, &scene);
            if let Some(draft) = self.drafts.get_mut(index) {
                draft.dirty = true;
                draft.error = Some(message.clone());
            }
            return Some(message);
        }
        state.set_source_scene(scene);
        if let Some(draft) = self.drafts.get_mut(index) {
            draft.committed = source;
            draft.error = None;
            draft.dirty = false;
        }
        None
    }

    pub fn set_softening(&mut self, state: &mut State, value: f64) -> Result<(), String> {
        let mut scene = state.sources.clone();
        scene
            .set_softening(value)
            .map_err(|error| error_message(&error))?;
        state.set_source_scene(scene);
        Ok(())
    }

    pub fn set_allow_inaccurate(&mut self, state: &mut State, allowed: bool) {
        let mut scene = state.sources.clone();
        scene.set_allow_inaccurate(allowed);
        state.set_source_scene(scene);
    }

    fn add(&mut self, state: &mut State) {
        let mut scene = state.sources.clone();
        if scene.add(self.add_kind.default_source()).is_ok() {
            state.set_source_scene(scene);
            self.sync(&state.sources);
        }
    }

    fn remove(&mut self, index: usize, state: &mut State) {
        let mut scene = state.sources.clone();
        if scene.remove(index).is_some() {
            state.set_source_scene(scene);
            self.sync(&state.sources);
        }
    }

    fn reset_entry(&mut self, index: usize, state: &mut State) {
        let mut scene = state.sources.clone();
        if scene.reset_entry(index) {
            state.set_source_scene(scene);
            if let Some(entry) = state.sources.entries.get(index) {
                self.drafts[index] = SourceDraft::from_entry(entry);
            }
        }
    }
}

/// The one-line viewport/status summary. `None` when no enabled family has
/// anything to report (the panel still shows a neutral status).
pub fn status_text(sampling: &SourceSampling) -> Option<String> {
    if let Some(error) = &sampling.validation_error {
        return Some(format!("Sources: {error}"));
    }
    let mut parts = Vec::new();
    for (name, family) in [("E", &sampling.electric), ("B", &sampling.magnetic)] {
        family_status_text(name, family, &mut parts);
    }
    (!parts.is_empty()).then(|| format!("Status: {}", parts.join("; ")))
}

fn family_status_text(name: &str, family: &SourceFamilyStatus, parts: &mut Vec<String>) {
    if family.sources == 0 || family.empty {
        return;
    }
    if family.accuracy_not_met_drawn > 0 {
        parts.push(format!(
            "{name} {} accuracy not met shown",
            family.accuracy_not_met_drawn
        ));
    } else if family.accuracy_not_met_total > 0 {
        parts.push(format!(
            "{name} {} accuracy not met hidden",
            family.accuracy_not_met_total
        ));
    }
    if family.unavailable > 0 {
        parts.push(format!("{name} {} unavailable", family.unavailable));
    }
}

/// The `Both` display legend segments: each family's label and its effective
/// arrow colour, in painting order. Single-family displays use the shared
/// colormap instead, so they have no family-colour legend.
pub fn both_legend(colors: FamilyColors) -> [(&'static str, [u8; 3]); 2] {
    [("E", colors.electric), ("B", colors.magnetic)]
}

/// Paint the `Both` display legend as one line at `pos`: the family labels in
/// their effective colours, separated by a neutral dot. The viewport overlay
/// and tests share this exact painting path.
pub fn paint_both_legend(painter: &egui::Painter, pos: egui::Pos2, colors: FamilyColors) {
    let font = egui::FontId::proportional(12.0);
    let neutral = egui::Color32::from_gray(220);
    let mut x = pos.x;
    let mut draw = |text: &str, color: egui::Color32| {
        let galley = painter.layout_no_wrap(text.to_owned(), font.clone(), color);
        let width = galley.size().x;
        painter.galley(egui::pos2(x, pos.y), galley, color);
        x += width;
    };
    draw("Both: ", neutral);
    for (index, (label, rgb)) in both_legend(colors).into_iter().enumerate() {
        if index > 0 {
            draw(" · ", neutral);
        }
        draw(label, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
    }
}

/// Draw the Sources panel. `compact` applies the same finger-sized row floor
/// as the other compact sheets; wide keeps the historical panel spacing.
pub fn show(ui: &mut egui::Ui, state: &mut State, panel: &mut SourcesPanelState, compact: bool) {
    if compact {
        ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
        ui.spacing_mut().item_spacing.y = crate::ui::layout::COMPACT_ITEM_SPACING;
    }
    panel.sync(&state.sources);
    panel.sync_densities(state);

    ui.horizontal(|ui| {
        ui.heading("Sources");
        if ui
            .button("Reset scene")
            .on_hover_text("Remove every source and restore the default scene")
            .clicked()
        {
            state.reset_sources();
            panel.sync(&state.sources);
        }
    });

    if let Some(notice) = state.source_notice.clone() {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(ui.visuals().warn_fg_color, &notice);
            if ui.button("Dismiss").clicked() {
                state.source_notice = None;
            }
        });
    }

    show_display(ui, state, panel);
    show_add(ui, state, panel);

    if state.sources.entries.is_empty() {
        ui.label("No sources yet. Add one to sample a field.");
    }

    let mut remove = None;
    let mut reset = None;
    let mut commit = None;
    for index in 0..state.sources.entries.len() {
        let Some(draft) = panel.drafts.get_mut(index) else {
            continue;
        };
        ui.push_id(index, |ui| {
            let tag = draft.tag();
            egui::CollapsingHeader::new(format!("{} {}", tag.label(), index + 1))
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut draft.enabled, "enabled").changed() {
                            commit = Some(index);
                        }
                        if ui.button("Remove source").clicked() {
                            remove = Some(index);
                        }
                    });
                    show_center(ui, draft, &mut commit, index);
                    if uses_axis(tag) {
                        show_axis(ui, draft, &mut commit, index);
                    }
                    show_parameters(ui, draft, tag, &mut commit, index);
                    if let Some(error) = &draft.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if ui.button("Reset this source").clicked() {
                        reset = Some(index);
                    }
                });
        });
    }
    if let Some(index) = remove {
        panel.remove(index, state);
    } else if let Some(index) = reset {
        panel.reset_entry(index, state);
    } else if let Some(index) = commit {
        panel.commit(index, state);
    }

    advanced(ui, state, panel);
    show_density_section(ui, state, panel);

    ui.label("Streamlines and particles use the formula field; switch to Formula to use them.");
}

fn show_display(ui: &mut egui::Ui, state: &mut State, panel: &mut SourcesPanelState) {
    ui.horizontal(|ui| {
        for (display, label) in [
            (SourceDisplay::Electric, "Electric"),
            (SourceDisplay::Magnetic, "Magnetic"),
            (SourceDisplay::Both, "Both"),
        ] {
            if ui
                .selectable_label(state.sources.display == display, label)
                .clicked()
                && state.sources.display != display
            {
                let mut scene = state.sources.clone();
                scene.set_display(display);
                state.set_source_scene(scene);
                panel.sync(&state.sources);
            }
        }
    });
    if state.sources.display == SourceDisplay::Both {
        let colors = state.source_family_colors();
        ui.horizontal(|ui| {
            for (label, rgb) in both_legend(colors) {
                ui.colored_label(egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]), label);
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("Object style");
        for (style, label) in [
            (ObjectStyle::Solid, "Solid"),
            (ObjectStyle::Wireframe, "Wireframe"),
        ] {
            if ui
                .selectable_label(state.object_style() == style, label)
                .clicked()
            {
                state.set_object_style(style);
            }
        }
    });
    ui.label(OBJECT_STYLE_HELP);
}

fn show_add(ui: &mut egui::Ui, state: &mut State, panel: &mut SourcesPanelState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Add source");
        egui::ComboBox::from_id_salt("source_add_kind")
            .selected_text(panel.add_kind.label())
            .show_ui(ui, |ui| {
                for tag in SourceKindTag::ALL {
                    ui.selectable_value(&mut panel.add_kind, tag, tag.label());
                }
            });
        let at_limit = state.sources.entries.len() >= MAX_SOURCES;
        if ui
            .add_enabled(!at_limit, egui::Button::new("Add"))
            .on_hover_text(format!("Add up to {MAX_SOURCES} sources"))
            .clicked()
        {
            panel.add(state);
        }
        ui.label(format!("{} / {MAX_SOURCES}", state.sources.entries.len()));
    });
}

fn show_center(
    ui: &mut egui::Ui,
    draft: &mut SourceDraft,
    commit: &mut Option<usize>,
    index: usize,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Center");
        for axis in ["x", "y", "z"] {
            ui.label(axis);
            let slot = match axis {
                "x" => &mut draft.center[0],
                "y" => &mut draft.center[1],
                _ => &mut draft.center[2],
            };
            if ui.add(egui::DragValue::new(slot).speed(0.1)).changed() {
                *commit = Some(index);
            }
        }
    });
}

fn show_axis(ui: &mut egui::Ui, draft: &mut SourceDraft, commit: &mut Option<usize>, index: usize) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Axis");
        for axis in ["x", "y", "z"] {
            ui.label(axis);
            let slot = match axis {
                "x" => &mut draft.axis[0],
                "y" => &mut draft.axis[1],
                _ => &mut draft.axis[2],
            };
            if ui.add(egui::DragValue::new(slot).speed(0.1)).changed() {
                *commit = Some(index);
            }
        }
    });
}

fn show_parameters(
    ui: &mut egui::Ui,
    draft: &mut SourceDraft,
    tag: SourceKindTag,
    commit: &mut Option<usize>,
    index: usize,
) {
    let strength = match tag.family() {
        crate::sources::FieldFamily::Electric => "Charge",
        crate::sources::FieldFamily::Magnetic => "Current",
    };
    ui.horizontal(|ui| {
        ui.label(strength);
        if ui
            .add(egui::DragValue::new(&mut draft.strength).speed(0.1))
            .changed()
        {
            *commit = Some(index);
        }
    });

    let has_radius = matches!(
        tag,
        SourceKindTag::ChargedRing
            | SourceKindTag::ChargedDisc
            | SourceKindTag::SolidSphere
            | SourceKindTag::CurrentLoop
            | SourceKindTag::Solenoid
    );
    if has_radius {
        ui.horizontal(|ui| {
            ui.label("Radius");
            if ui
                .add(
                    egui::DragValue::new(&mut draft.radius)
                        .speed(0.05)
                        .range(0.0..=f64::INFINITY),
                )
                .changed()
            {
                *commit = Some(index);
            }
        });
    }

    if matches!(
        tag,
        SourceKindTag::FiniteLineCharge | SourceKindTag::Solenoid
    ) {
        ui.horizontal(|ui| {
            ui.label("Length");
            if ui
                .add(
                    egui::DragValue::new(&mut draft.length)
                        .speed(0.05)
                        .range(0.0..=f64::INFINITY),
                )
                .changed()
            {
                *commit = Some(index);
            }
        });
    }

    if matches!(tag, SourceKindTag::Solenoid) {
        ui.horizontal(|ui| {
            ui.label("Turns");
            if ui
                .add(
                    egui::DragValue::new(&mut draft.turns)
                        .speed(1.0)
                        .range(0..=u32::MAX),
                )
                .changed()
            {
                *commit = Some(index);
            }
        });
    }
}

fn default_density(name: String) -> DensitySpec {
    DensitySpec {
        name,
        family: DensityFamily::VolumeCharge,
        support: DensitySupport {
            kind: SupportKind::Sphere,
            center: glam::Vec3::ZERO,
            axis: glam::Vec3::Z,
            size: glam::Vec3::splat(2.0),
        },
        formula: "1".to_string(),
        samples: 12,
        softening: 0.0,
        time_law: TimeLaw::Steady,
        charge_formula: None,
    }
}

#[doc(hidden)]
pub fn density_report_lines(row: &DensityReportRow) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(charge) = &row.charge {
        lines.push(charge_line(charge));
    }
    if let Some(continuity) = &row.continuity {
        lines.push(continuity_line(continuity));
    }
    lines
}

fn charge_line(report: &ChargeReport) -> String {
    match &report.status {
        DiagnosticsStatus::Estimate { error } if report.total.is_finite() && error.is_finite() => {
            format!(
                "Estimate: total = {:.6e}, error = {:.6e}",
                report.total, error
            )
        }
        DiagnosticsStatus::Estimate { .. } => {
            "Unresolved: the charge estimate was not finite".to_string()
        }
        DiagnosticsStatus::Unresolved { reason } => format!("Unresolved: {reason}"),
        DiagnosticsStatus::Violation { reason } => format!("Violation: {reason}"),
    }
}

fn continuity_line(report: &ContinuityReport) -> String {
    match &report.status {
        ContinuityStatus::Estimate { value, error }
            if value.is_finite()
                && error.is_finite()
                && report.div_j_min.is_finite()
                && report.div_j_max.is_finite() =>
        {
            format!(
                "Estimate: div J in [{:.6e}, {:.6e}], residual = {value:.6e}, error = {error:.6e}",
                report.div_j_min, report.div_j_max
            )
        }
        ContinuityStatus::Estimate { .. } => {
            "Unresolved: the continuity estimate was not finite".to_string()
        }
        ContinuityStatus::Unresolved { reason } => format!("Unresolved: {reason}"),
        ContinuityStatus::Violation {
            residual,
            tolerance,
        } if residual.is_finite() && tolerance.is_finite() => {
            format!("Violation: residual = {residual:.6e}, tolerance = {tolerance:.6e}")
        }
        ContinuityStatus::Violation { .. } => {
            "Unresolved: the continuity violation was not finite".to_string()
        }
    }
}

fn show_density_section(ui: &mut egui::Ui, state: &mut State, panel: &mut SourcesPanelState) {
    egui::CollapsingHeader::new("Custom densities")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Normalized teaching values (eps0 = mu0 = 1). A custom density does not \
                 feed the sampled E/B fields in this slice. SurfaceCharge has no volume total.",
            );
            ui.label("Density outside the support is identically zero.");

            let at_cap = state.densities().len() >= MAX_DENSITIES;
            let mut add = false;
            let mut compute = false;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!at_cap, egui::Button::new("Add density"))
                    .on_hover_text(format!("Add up to {MAX_DENSITIES} custom densities"))
                    .clicked()
                {
                    add = true;
                }
                if ui
                    .button("Compute")
                    .on_hover_text(
                        "Sample the declared lattice and run the applicable diagnostics once",
                    )
                    .clicked()
                {
                    compute = true;
                }
                ui.label(format!("{} / {MAX_DENSITIES}", state.densities().len()));
                if at_cap {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("At most {MAX_DENSITIES} custom densities."),
                    );
                }
            });
            if add {
                panel.add_density(state);
            }
            if compute {
                state.refresh_density_reports();
            }
            if let Some(error) = panel.density_error.clone() {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }

            let mut remove = None;
            let mut immediate = Vec::new();
            let mut pending = Vec::new();
            // Continuous widgets (sliders and drag values) only commit once,
            // when the pointer gesture ends. Committing per frame would
            // validate and re-sample the whole lattice on every frame, and a
            // drag still in flight must survive another row's immediate edit.
            let pointer_down = ui.input(|input| input.pointer.any_down());
            for index in 0..panel.density_drafts.len() {
                let Some(draft) = panel.density_drafts.get_mut(index) else {
                    continue;
                };
                let mut edited = false;
                let mut continuous = false;
                ui.push_id(("custom_density", index), |ui| {
                    egui::CollapsingHeader::new(format!("{} {}", draft.family.label(), index + 1))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Name");
                                edited |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut draft.name)
                                            .desired_width(140.0),
                                    )
                                    .changed();
                                let mut family = draft.family;
                                egui::ComboBox::from_id_salt(("density_family", index))
                                    .selected_text(family.label())
                                    .show_ui(ui, |ui| {
                                        for option in [
                                            DensityFamily::VolumeCharge,
                                            DensityFamily::SurfaceCharge,
                                            DensityFamily::SteadyCurrent,
                                        ] {
                                            ui.selectable_value(
                                                &mut family,
                                                option,
                                                option.label(),
                                            );
                                        }
                                    });
                                if family != draft.family {
                                    draft.family = family;
                                    edited = true;
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Support");
                                let mut kind = draft.support_kind;
                                egui::ComboBox::from_id_salt(("density_support", index))
                                    .selected_text(kind.label())
                                    .show_ui(ui, |ui| {
                                        for option in [
                                            SupportKind::Sphere,
                                            SupportKind::Box,
                                            SupportKind::Cylinder,
                                        ] {
                                            ui.selectable_value(&mut kind, option, option.label());
                                        }
                                    });
                                if kind != draft.support_kind {
                                    draft.support_kind = kind;
                                    edited = true;
                                }
                                ui.label("Samples");
                                let response = ui.add(
                                    egui::Slider::new(
                                        &mut draft.samples,
                                        MIN_SAMPLES..=MAX_SAMPLES,
                                    )
                                    .integer(),
                                );
                                if response.changed() {
                                    continuous = true;
                                }
                            });
                            for (label, values) in [
                                ("Center", &mut draft.center),
                                ("Axis", &mut draft.axis),
                                ("Size", &mut draft.size),
                            ] {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(label);
                                    for (axis, slot) in ["x", "y", "z"].into_iter().zip(values) {
                                        ui.label(axis);
                                        let response =
                                            ui.add(egui::DragValue::new(slot).speed(0.05));
                                        if response.changed() {
                                            continuous = true;
                                        }
                                    }
                                });
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Formula");
                                edited |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut draft.formula)
                                            .hint_text(draft.family.formula_hint())
                                            .desired_width(220.0),
                                    )
                                    .changed();
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Time law");
                                let mut law_kind = draft.law_kind;
                                egui::ComboBox::from_id_salt(("density_time_law", index))
                                    .selected_text(law_kind.label())
                                    .show_ui(ui, |ui| {
                                        for option in [TimeLawKind::Steady, TimeLawKind::Harmonic] {
                                            ui.selectable_value(
                                                &mut law_kind,
                                                option,
                                                option.label(),
                                            );
                                        }
                                    });
                                if law_kind != draft.law_kind {
                                    draft.law_kind = law_kind;
                                    edited = true;
                                }
                                if draft.law_kind.is_harmonic() {
                                    ui.label("Omega (rad/s)");
                                    if ui
                                        .add(egui::DragValue::new(&mut draft.omega).speed(0.05))
                                        .changed()
                                    {
                                        continuous = true;
                                    }
                                    ui.label("Phase (rad)");
                                    if ui
                                        .add(egui::DragValue::new(&mut draft.phase).speed(0.01))
                                        .changed()
                                    {
                                        continuous = true;
                                    }
                                }
                            });
                            if draft.family == DensityFamily::SteadyCurrent {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Charge formula");
                                    edited |= ui
                                        .add(
                                            egui::TextEdit::singleline(&mut draft.charge_formula)
                                                .hint_text("scalar rho(x, y, z, t)")
                                                .desired_width(220.0),
                                        )
                                        .changed();
                                });
                                if !draft.law_kind.is_harmonic()
                                    && !draft.charge_formula.trim().is_empty()
                                {
                                    ui.label(
                                        "Charge formula is ignored while the time law is \
                                         steady.",
                                    );
                                }
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Softening");
                                let response = ui.add(
                                    egui::DragValue::new(&mut draft.softening)
                                        .speed(0.005)
                                        .range(0.0..=1.0),
                                );
                                if response.changed() {
                                    continuous = true;
                                }
                                if ui.button("Remove density").clicked() {
                                    remove = Some(index);
                                }
                            });
                            if let Some(error) = &draft.error {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            }
                        });
                });
                if edited || continuous {
                    // Stop `sync_densities` from replacing this frame's mirror
                    // with the committed value while another row commits.
                    draft.dirty = true;
                }
                if edited {
                    immediate.push(index);
                } else if continuous {
                    if pointer_down {
                        pending.push(index);
                    } else {
                        // Keyboard edit: no gesture to wait for.
                        immediate.push(index);
                    }
                }
                if let Some(row) = state
                    .density_reports()
                    .iter()
                    .find(|row| row.spec_index == index)
                {
                    for line in density_report_lines(row) {
                        ui.label(line);
                    }
                    if row.continuity.is_some() {
                        ui.label(
                            "Continuity is measured in normalized teaching units \
                             (eps0 = mu0 = 1).",
                        );
                    }
                }
                if draft.family == DensityFamily::SurfaceCharge {
                    ui.label("No volume total for a SurfaceCharge density.");
                }
            }
            for index in pending {
                panel.queue_pending(index);
            }
            // Text/combo edits commit their whole mirror now, so any deferred
            // gesture on the same row is superseded.
            for &index in &immediate {
                panel.forget_pending(index);
            }
            let mut commits = if pointer_down {
                Vec::new()
            } else {
                // The gesture ended (or its release was missed): commit every
                // deferred continuous edit once, in first-changed order.
                panel.take_pending()
            };
            commits.retain(|index| *index < panel.density_drafts.len());
            for index in immediate {
                if !commits.contains(&index) {
                    commits.push(index);
                }
            }
            if let Some(index) = remove {
                // Removing one row cancels only its own deferred edit; the
                // other rows commit before the structural rebuild replaces
                // their draft mirrors.
                for commit_index in commits.into_iter().filter(|&commit| commit != index) {
                    panel.commit_density(commit_index, state);
                }
                panel.remove_density(index, state);
            } else {
                for index in commits {
                    panel.commit_density(index, state);
                }
            }

            ui.separator();
            ui.label(
                "Reports refresh only on demand: press Compute or accept an edit. \
                 A parameter change invalidates them.",
            );
        });
}

fn advanced(ui: &mut egui::Ui, state: &mut State, panel: &mut SourcesPanelState) {
    egui::CollapsingHeader::new("Advanced")
        .default_open(true)
        .show(ui, |ui| {
            let mut softening = state.sources.softening;
            ui.horizontal(|ui| {
                ui.label("Softening");
                if ui
                    .add(
                        egui::DragValue::new(&mut softening)
                            .speed(0.001)
                            .range(MIN_SOFTENING..=MAX_SOFTENING),
                    )
                    .changed()
                {
                    let _ = panel.set_softening(state, softening);
                }
                ui.label(format!("{MIN_SOFTENING} .. {MAX_SOFTENING}"));
            });
            ui.label(
                "Model note: softening is part of the teaching model; smaller values sharpen \
                 near-source peaks, larger values smooth them.",
            );
            let mut allow = state.sources.allow_inaccurate;
            if ui
                .checkbox(
                    &mut allow,
                    "Show finite results labelled \"accuracy not met\"",
                )
                .changed()
            {
                panel.set_allow_inaccurate(state, allow);
            }
            let status = state
                .source_sampling()
                .and_then(status_text)
                .unwrap_or_else(|| "Status: no source results sampled yet".to_owned());
            ui.label(status);
        });
}

fn uses_axis(tag: SourceKindTag) -> bool {
    !matches!(tag, SourceKindTag::PointCharge | SourceKindTag::SolidSphere)
}
