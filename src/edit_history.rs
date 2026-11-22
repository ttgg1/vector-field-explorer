//! Bounded, in-memory edit history (issue #86 slice A1, named entries #231).
//!
//! A history stores snapshots of the editable document, not runtime state. Each
//! gesture opens a transaction with [`EditHistory::begin`], records at most one
//! entry on [`EditHistory::commit`], and moves a single cursor through the undo
//! and redo stacks on [`EditHistory::undo`] / [`EditHistory::redo`]. Every entry
//! also carries a human label describing the edit that produced it, so the
//! next undo/redo can be named. Nothing here touches the app, the renderer,
//! persistence or input; slice A2 wires it in. See
//! `docs/superpowers/specs/2026-11-18-edit-history-contract.md`.

use crate::appearance_profile::AppearanceProfile;
use crate::parameters::ParameterSet;
use crate::persistence::{ProbeLoopRecord, ProbeSurfaceRecord, ViewRecord};
use crate::scalar_view::ProbeSet;
use crate::scene_document::SceneCamera;

/// Recording past this many entries evicts the oldest one.
pub const MAX_HISTORY: usize = 64;

/// The honest fallback when an edit cannot be named confidently.
pub const GENERIC_LABEL: &str = "edit";

/// One explicit reset scope. Only runtime state (`Simulation`) is outside the
/// document snapshot, so only that scope is undo-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditScope {
    /// Colour mapping: palette, ramp, scale mode, quantity and range.
    Color,
    /// The orbit/2D camera pose.
    Camera,
    /// Runtime state: simulation time, particle trails/dots, click bursts.
    Simulation,
    /// The whole document: view fields plus appearance.
    All,
}

impl EditScope {
    /// Every scope, in menu order.
    pub const ALL: [Self; 4] = [Self::Color, Self::Camera, Self::Simulation, Self::All];

    /// The user-facing name, matching the menu label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Color => "Color",
            Self::Camera => "Camera",
            Self::Simulation => "Simulation",
            Self::All => "All",
        }
    }

    /// Whether resetting this scope is recorded as one history entry.
    ///
    /// Runtime state is not part of the document snapshot, so the simulation
    /// reset is an explicit undo-free action.
    pub fn records_history(self) -> bool {
        !matches!(self, Self::Simulation)
    }
}

/// The immutable document value stored in a history entry.
///
/// The appearance name is a sentinel: the snapshot carries the profile's style
/// data, not a user-facing name to restore.
#[derive(Debug, Clone, PartialEq)]
pub struct EditSnapshot {
    /// Formula, mode, plot settings, domain, density, arrows and colour.
    pub view: ViewRecord,
    /// Named parameter values.
    pub parameters: ParameterSet,
    /// Orbit/2D camera pose.
    pub camera: SceneCamera,
    /// Viewport style, layer styles and colour mapping.
    pub appearance: AppearanceProfile,
}

impl EditSnapshot {
    /// Build a snapshot from the four document parts.
    pub fn new(
        view: ViewRecord,
        parameters: ParameterSet,
        camera: SceneCamera,
        appearance: AppearanceProfile,
    ) -> Self {
        Self {
            view,
            parameters,
            camera,
            appearance,
        }
    }

    /// Whether the editable document (everything except the camera pose)
    /// changed from `self` to `after`.
    ///
    /// Camera orbit/pan/zoom is navigation, not a document edit, so the frame
    /// driver uses this to keep it outside the undo stack. Explicit reset
    /// scopes record through the same snapshot but supply their own label.
    pub fn document_changed(&self, after: &Self) -> bool {
        self.view != after.view
            || self.parameters != after.parameters
            || self.appearance != after.appearance
    }

    /// A short, honest label for the edit from `self` to `after`.
    ///
    /// The label names the first changed document field group in a fixed
    /// precedence, so a formula edit says `change formula`, a probe-surface
    /// move says `move probe surface`, and so on. An unrecognised change falls
    /// back to [`GENERIC_LABEL`] rather than inventing a wrong name.
    pub fn describe_change(&self, after: &Self) -> String {
        if self.view.formula != after.view.formula {
            return "change formula".to_string();
        }
        if self.view.field_mode != after.view.field_mode {
            return "change field mode".to_string();
        }
        if self.view.plot_kind != after.view.plot_kind
            || self.view.curve_source != after.view.curve_source
            || self.view.surface_source != after.view.surface_source
        {
            return "change plot".to_string();
        }
        if self.parameters != after.parameters {
            return "change parameters".to_string();
        }
        if let Some(label) = surface_change(&self.view.probe_surfaces, &after.view.probe_surfaces) {
            return label.to_string();
        }
        if let Some(label) = loop_change(&self.view.probe_loops, &after.view.probe_loops) {
            return label.to_string();
        }
        if let Some(label) = probe_set_change(&self.view.probes, &after.view.probes) {
            return label.to_string();
        }
        if self.view.probe_family != after.view.probe_family {
            return "change probe field".to_string();
        }
        if self.view.density != after.view.density {
            return "change density".to_string();
        }
        if self.view.domain_min != after.view.domain_min
            || self.view.domain_max != after.view.domain_max
        {
            return "change domain".to_string();
        }
        if colour_changed(&self.view, &after.view) {
            return "change colour mapping".to_string();
        }
        if arrows_changed(&self.view, &after.view) {
            return "change arrows".to_string();
        }
        if streamlines_changed(&self.view, &after.view) {
            return "change streamlines".to_string();
        }
        if self.view.show_grid != after.view.show_grid
            || self.view.show_coordinate_system != after.view.show_coordinate_system
        {
            return "change reference geometry".to_string();
        }
        if self.view.particles != after.view.particles
            || self.view.particle_size != after.view.particle_size
            || self.view.particle_count != after.view.particle_count
            || self.view.left_click_action != after.view.left_click_action
        {
            return "change particles".to_string();
        }
        if self.view != after.view {
            return "change view settings".to_string();
        }
        if self.appearance != after.appearance {
            return "change appearance".to_string();
        }
        if self.camera != after.camera {
            return "change camera".to_string();
        }
        GENERIC_LABEL.to_string()
    }
}

/// Name the change between two Gaussian-surface lists.
fn surface_change(
    before: &[ProbeSurfaceRecord],
    after: &[ProbeSurfaceRecord],
) -> Option<&'static str> {
    if after.len() > before.len() {
        return Some("add probe surface");
    }
    if after.len() < before.len() {
        return Some("remove probe surface");
    }
    for (old, new) in before.iter().zip(after) {
        if old == new {
            continue;
        }
        if old.kind != new.kind || old.segments != new.segments {
            return Some("change probe surface");
        }
        if old.center != new.center || old.axis != new.axis {
            return Some("move probe surface");
        }
        if old.size != new.size {
            return Some(if old.kind == "sphere" {
                "change probe radius"
            } else {
                "resize probe surface"
            });
        }
        return Some("change probe surface");
    }
    None
}

/// Name the change between two circulation-loop lists.
fn loop_change(before: &[ProbeLoopRecord], after: &[ProbeLoopRecord]) -> Option<&'static str> {
    if after.len() > before.len() {
        return Some("add circulation loop");
    }
    if after.len() < before.len() {
        return Some("remove circulation loop");
    }
    for (old, new) in before.iter().zip(after) {
        if old == new {
            continue;
        }
        if old.kind != new.kind || old.segments != new.segments {
            return Some("change circulation loop");
        }
        if old.center != new.center || old.axis != new.axis {
            return Some("move circulation loop");
        }
        if old.size != new.size {
            return Some("resize circulation loop");
        }
        return Some("change circulation loop");
    }
    None
}

/// Name the change between two point/line probe sets.
fn probe_set_change(before: &ProbeSet, after: &ProbeSet) -> Option<&'static str> {
    if after.points.len() > before.points.len() {
        return Some("add probe");
    }
    if after.points.len() < before.points.len() {
        return Some("remove probe");
    }
    if after.lines.len() > before.lines.len() {
        return Some("add line probe");
    }
    if after.lines.len() < before.lines.len() {
        return Some("remove line probe");
    }
    if before.points != after.points {
        return Some("move probe");
    }
    if before.lines != after.lines {
        return Some("change line probe");
    }
    None
}

/// Whether the colour mapping fields (not the rest of the view) changed.
fn colour_changed(before: &ViewRecord, after: &ViewRecord) -> bool {
    before.colormap != after.colormap
        || before.color_ramp != after.color_ramp
        || before.color_mode != after.color_mode
        || before.color_quantity != after.color_quantity
        || before.auto_fit != after.auto_fit
        || before.color_min != after.color_min
        || before.color_max != after.color_max
}

/// Whether the arrow fields changed.
fn arrows_changed(before: &ViewRecord, after: &ViewRecord) -> bool {
    before.arrows != after.arrows
        || before.arrow_style != after.arrow_style
        || before.arrow_style_2d != after.arrow_style_2d
        || before.arrow_params != after.arrow_params
        || before.arrow_scale != after.arrow_scale
        || before.arrow_opacity != after.arrow_opacity
        || before.arrow_length_mode != after.arrow_length_mode
        || before.magnitude_scale != after.magnitude_scale
        || before.proportional_exponent != after.proportional_exponent
}

/// Whether the streamline or seed fields changed.
fn streamlines_changed(before: &ViewRecord, after: &ViewRecord) -> bool {
    before.streamlines != after.streamlines
        || before.seed_mode != after.seed_mode
        || before.streamline_count != after.streamline_count
        || before.seed_layout != after.seed_layout
}

/// One undo-stack entry: the snapshot and the label of the edit it belongs to.
///
/// A past entry stores the pre-edit snapshot together with the edit's label. A
/// future (redo) entry stores the post-edit snapshot with the same label.
#[derive(Debug, Clone, PartialEq)]
struct Entry<S> {
    snapshot: S,
    label: String,
}

/// A bounded undo/redo history with one open transaction at most.
///
/// The undo stack is capped at [`MAX_HISTORY`]; recording past the cap evicts
/// the oldest entry. The redo stack never outgrows the undo stack. Each entry
/// carries the human label of the edit that produced it, so the next undo and
/// redo can be named without a second history.
#[derive(Debug, Clone, PartialEq)]
pub struct EditHistory<S> {
    /// Pre-edit snapshots for each recorded edit, oldest first.
    past: Vec<Entry<S>>,
    /// Post-edit snapshots returned by undo, most recently undone last.
    future: Vec<Entry<S>>,
    /// The pre-edit snapshot of the open transaction, if any.
    pending: Option<S>,
}

impl<S: Clone + PartialEq> EditHistory<S> {
    /// An empty history with no open transaction.
    pub fn new() -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            pending: None,
        }
    }

    /// The number of undo steps available.
    pub fn len(&self) -> usize {
        self.past.len()
    }

    /// The number of redo steps available.
    pub fn redo_len(&self) -> usize {
        self.future.len()
    }

    /// Whether there is nothing to undo.
    pub fn is_empty(&self) -> bool {
        self.past.is_empty()
    }

    /// Whether at least one undo step is available.
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Whether at least one redo step is available.
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// The label of the next undo, when one is available.
    pub fn undo_label(&self) -> Option<&str> {
        self.past.last().map(|entry| entry.label.as_str())
    }

    /// The label of the next redo, when one is available.
    ///
    /// Undo pushes the applied edit's label onto the redo stack, so this names
    /// the edit that will be reapplied.
    pub fn redo_label(&self) -> Option<&str> {
        self.future.last().map(|entry| entry.label.as_str())
    }

    /// Whether a transaction is open.
    pub fn is_recording(&self) -> bool {
        self.pending.is_some()
    }

    /// Capture the pre-edit snapshot once.
    ///
    /// A second `begin` during the same gesture keeps the first snapshot, so a
    /// gesture cannot nest accidentally.
    pub fn begin(&mut self, before: S) {
        if self.pending.is_none() {
            self.pending = Some(before);
        }
    }

    /// Record one entry if the document changed, then end the transaction.
    ///
    /// This is the unnamed form; the entry falls back to [`GENERIC_LABEL`].
    /// See [`commit_named`](Self::commit_named) for the labelled seam the frame
    /// driver uses.
    pub fn commit(&mut self, after: &S) -> bool {
        self.commit_named(after, GENERIC_LABEL)
    }

    /// Record one labelled entry if the document changed, then end the
    /// transaction.
    ///
    /// Returns `true` iff an entry was recorded. A commit without a `begin`
    /// records nothing. An unchanged document records nothing but still ends
    /// the transaction. A real change drops the redo tail and evicts the oldest
    /// entry at [`MAX_HISTORY`].
    pub fn commit_named(&mut self, after: &S, label: impl Into<String>) -> bool {
        let Some(before) = self.pending.take() else {
            return false;
        };
        if &before == after {
            return false;
        }
        self.push_past(before, label.into());
        self.future.clear();
        true
    }

    /// Discard the open transaction, leaving undo and redo untouched.
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// `begin` + `commit` in one call, with the same rules.
    ///
    /// When a transaction is already open, `record` still uses the original
    /// pending snapshot from [`begin`](Self::begin) as the pre-edit state; the
    /// `before` argument is ignored. A mid-transaction record therefore folds
    /// into the gesture's single undo step instead of opening a second one.
    pub fn record(&mut self, before: S, after: &S) -> bool {
        self.record_named(before, after, GENERIC_LABEL)
    }

    /// `begin` + [`commit_named`](Self::commit_named) in one call.
    pub fn record_named(&mut self, before: S, after: &S, label: impl Into<String>) -> bool {
        self.begin(before);
        self.commit_named(after, label)
    }

    /// Move back one entry: push `current` onto redo and return the previous
    /// snapshot. Returns `None` and changes nothing when there is nothing to
    /// undo.
    ///
    /// The undone edit's label travels with `current` onto the redo stack.
    pub fn undo(&mut self, current: S) -> Option<S> {
        let entry = self.past.pop()?;
        self.future.push(Entry {
            snapshot: current,
            label: entry.label.clone(),
        });
        Some(entry.snapshot)
    }

    /// Move forward one entry: push `current` onto undo and return the next
    /// snapshot. Returns `None` and changes nothing when there is nothing to
    /// redo.
    pub fn redo(&mut self, current: S) -> Option<S> {
        let entry = self.future.pop()?;
        self.push_past(current, entry.label.clone());
        Some(entry.snapshot)
    }

    /// Drop every entry, both stacks and any open transaction.
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
        self.pending = None;
    }

    /// Push onto the bounded undo stack, evicting the oldest entry when full.
    fn push_past(&mut self, snapshot: S, label: String) {
        if self.past.len() >= MAX_HISTORY {
            self.past.remove(0);
        }
        self.past.push(Entry { snapshot, label });
    }
}

impl<S: Clone + PartialEq> Default for EditHistory<S> {
    fn default() -> Self {
        Self::new()
    }
}
