//! The dedicated Sources panel and its draft/commit seam.
//!
//! The panel edits a list of [`SourceDraft`]s. A draft keeps the user's text
//! even when it is invalid; only a valid edit is handed to
//! [`State::set_source_scene`], so the committed scene and the saved payload
//! never see a rejected value. The viewport status is derived from the cached
//! [`SourceSampling`] rather than inventing a second accuracy accounting.

use eframe::egui;
use glam::DVec3;

use crate::source_scene::{
    MAX_SOFTENING, MIN_SOFTENING, SourceDisplay, SourceEntry, SourceFamilyStatus, SourceKindTag,
    SourceSampling, SourceScene, error_message, scene_error_message, validate_source,
};
use crate::sources::{MAX_SOURCES, Source, SourceKind};
use crate::viewmodel::State;

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

/// Panel-local state: the kind the Add control will create and one draft per
/// committed scene entry.
#[derive(Debug)]
pub struct SourcesPanelState {
    pub add_kind: SourceKindTag,
    pub drafts: Vec<SourceDraft>,
}

impl Default for SourcesPanelState {
    fn default() -> Self {
        Self {
            add_kind: SourceKindTag::PointCharge,
            drafts: Vec::new(),
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

/// The `Both` display legend segments: each family's label and its fixed
/// arrow colour, in painting order. Single-family displays use the shared
/// colormap instead, so they have no fixed-colour legend.
pub fn both_legend() -> [(&'static str, [u8; 3]); 2] {
    [
        ("E amber", crate::source_scene::ELECTRIC_COLOR),
        ("B cyan", crate::source_scene::MAGNETIC_COLOR),
    ]
}

/// Paint the `Both` display legend as one line at `pos`: the family labels in
/// their fixed colours, separated by a neutral dot. The viewport overlay and
/// tests share this exact painting path.
pub fn paint_both_legend(painter: &egui::Painter, pos: egui::Pos2) {
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
    for (index, (label, rgb)) in both_legend().into_iter().enumerate() {
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
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(
                    crate::source_scene::ELECTRIC_COLOR[0],
                    crate::source_scene::ELECTRIC_COLOR[1],
                    crate::source_scene::ELECTRIC_COLOR[2],
                ),
                "E amber",
            );
            ui.colored_label(
                egui::Color32::from_rgb(
                    crate::source_scene::MAGNETIC_COLOR[0],
                    crate::source_scene::MAGNETIC_COLOR[1],
                    crate::source_scene::MAGNETIC_COLOR[2],
                ),
                "B cyan",
            );
        });
    }
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
