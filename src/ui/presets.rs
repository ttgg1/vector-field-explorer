use eframe::egui;
use log::{debug, warn};
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::export::CameraPreset;
#[cfg(not(target_arch = "wasm32"))]
use crate::export::{delete_preset, load_presets, save_preset};
#[cfg(target_arch = "wasm32")]
use crate::export::{load_wasm_presets, save_wasm_presets};
use crate::plot::PlotKind;
use crate::preset_library::{PresetEntry, PresetFilters, PresetGroup, PresetLibrary};
use crate::presets::{Preset, START_HERE, preset_approximation_label, presets};
use crate::render::Camera;
use crate::source_presets::source_preset;
use crate::source_scene::FieldMode;
use crate::view_command::ViewCommand;
use crate::viewmodel::State;

/// Directory where presets.json lives by default: XDG_CONFIG_HOME if set,
/// otherwise ~/.config/vector-field-explorer.
#[cfg(not(target_arch = "wasm32"))]
pub fn default_preset_dir() -> PathBuf {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(p) => PathBuf::from(p).join("vector-field-explorer"),
        Err(_) => {
            let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".config/vector-field-explorer")
        }
    }
}

/// The camera action buttons the panel offers, in display order. The labels
/// are pinned by the tests and the commands go through the same
/// [`crate::view_command::apply_view_command`] seam as `R` and `F`.
pub const CAMERA_BUTTONS: &[(&str, ViewCommand)] = &[
    ("Reset view", ViewCommand::Reset),
    ("Fit domain", ViewCommand::Fit),
];

/// The catalogue groups in display order. The library holds entries in
/// catalogue order inside each group; the panel renders these headers in this
/// same fixed order and skips a group with no visible entries.
const PRESET_GROUPS: [PresetGroup; 4] = [
    PresetGroup::Classic,
    PresetGroup::Physics,
    PresetGroup::Sources,
    PresetGroup::Gallery,
];

/// Index of `group` in [`PRESET_GROUPS`]; also the persisted expansion slot.
fn group_index(group: PresetGroup) -> usize {
    PRESET_GROUPS
        .iter()
        .position(|candidate| *candidate == group)
        .expect("every PresetGroup is listed")
}

/// Display heading for one catalogue group.
fn group_title(group: PresetGroup) -> &'static str {
    match group {
        PresetGroup::Classic => "Classic",
        PresetGroup::Physics => "Physics",
        PresetGroup::Sources => "Sources",
        PresetGroup::Gallery => "Gallery",
    }
}

/// The row label for a built-in formula preset. A 3D preset says so; a legacy
/// 2D physics formula carries the honest approximation note.
fn field_preset_label(preset: &Preset) -> String {
    if preset.dimension == 3 {
        return format!("{} (3D)", preset.name);
    }
    match preset_approximation_label(preset.name) {
        Some(note) => format!("{} ({note})", preset.name),
        None => preset.name.to_string(),
    }
}

/// The label a library entry shows: formula rows keep their 3D/approximation
/// annotation, source rows use the plain catalogue name (their summary follows
/// inline).
fn entry_label(entry: &PresetEntry) -> String {
    match entry.group {
        PresetGroup::Classic | PresetGroup::Physics => presets()
            .iter()
            .find(|preset| preset.name == entry.name)
            .map(field_preset_label)
            .unwrap_or_else(|| entry.name.to_string()),
        PresetGroup::Sources | PresetGroup::Gallery => entry.name.to_string(),
    }
}

/// UI panel for managing camera presets, with the built-in field presets
/// grouped by category above them. The panel's user-facing actions
/// (`save_current`, `load`, `delete`, `load_field`) are factored into plain
/// methods so the buttons in [`PresetPanel::show`] route through exactly the
/// behavior the integration tests in `tests/ui_preset_test.rs` exercise.
///
/// Ticket 14 additions: the row whose camera state was last saved/applied is
/// highlighted (`active`), and deleting takes a confirmation click
/// (`pending_delete`) instead of removing on the first click.
pub struct PresetPanel {
    /// Directory where presets.json lives.
    dir: PathBuf,
    /// Cached list of presets.
    presets: Vec<CameraPreset>,
    /// Pending name for the "Save current view" name field.
    pub new_name: String,
    /// The preset last saved or applied; drives the row highlight.
    active: Option<String>,
    /// The built-in field preset last clicked, so duplicate formulas can
    /// highlight only the row the user picked.
    active_field: Option<String>,
    /// The built-in source preset last clicked.
    active_source: Option<String>,
    /// Preset awaiting the delete confirmation.
    pending_delete: Option<String>,
    /// A camera loaded this frame, for the app to hand to the live view. Taken
    /// once so one click applies one camera.
    applied: Option<Camera>,
    /// One non-blocking note when browser storage cannot hold the list.
    pub storage_note: Option<String>,
    /// The merged built-in formula/source catalogue, indexed once.
    library: PresetLibrary,
    /// Session-only search text; trimmed by the library's search.
    query: String,
    /// Session-only dimension/animated filters.
    filters: PresetFilters,
    /// Whether each group's body is expanded, in [`PRESET_GROUPS`] order.
    expanded: [bool; 4],
    /// A user expansion toggle since the app last drained it.
    layout_changed: bool,
}

impl Default for PresetPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetPanel {
    /// Panel pointed at the default data directory on native, or at the
    /// browser's localStorage-backed list on wasm. Built-in field presets stay
    /// available on both.
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dir = default_preset_dir();
            if let Err(error) = fs::create_dir_all(&dir) {
                warn!(
                    "could not create preset directory {}: {error}",
                    dir.display()
                );
            }
            let presets = match load_presets(&dir) {
                Ok(presets) => presets,
                Err(error) => {
                    warn!("preset load failed from {}: {error}", dir.display());
                    Vec::new()
                }
            };
            Self::from_parts(dir, presets, None)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let (presets, storage_note) = load_wasm_presets();
            Self::from_parts(PathBuf::new(), presets, storage_note)
        }
    }

    fn from_parts(dir: PathBuf, presets: Vec<CameraPreset>, storage_note: Option<String>) -> Self {
        let library = PresetLibrary::builtin().unwrap_or_else(|error| {
            warn!("preset library could not be built: {error}");
            PresetLibrary::from_entries(Vec::new()).expect("an empty library has no duplicate IDs")
        });
        Self {
            dir,
            presets,
            new_name: String::new(),
            active: None,
            active_field: None,
            active_source: None,
            pending_delete: None,
            applied: None,
            storage_note,
            library,
            query: String::new(),
            filters: PresetFilters::default(),
            expanded: [true; 4],
            layout_changed: false,
        }
    }

    /// Panel pointed at an explicit native directory (used by tests).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_with_dir(dir: PathBuf) -> io::Result<Self> {
        if let Err(error) = fs::create_dir_all(&dir) {
            warn!(
                "could not create preset directory {}: {error}",
                dir.display()
            );
        }
        let presets = match load_presets(&dir) {
            Ok(presets) => presets,
            Err(error) => {
                warn!("preset load failed from {}: {error}", dir.display());
                Vec::new()
            }
        };
        Ok(Self::from_parts(dir, presets, None))
    }

    /// Write the browser list through to localStorage; a storage failure
    /// leaves the in-memory list usable and raises the one panel note.
    #[cfg(target_arch = "wasm32")]
    fn persist_wasm(&mut self) {
        match save_wasm_presets(&self.presets) {
            Ok(()) => self.storage_note = None,
            Err(error) => {
                warn!("preset save failed: {error}");
                self.storage_note = Some(crate::export::storage_note());
            }
        }
    }

    /// Directory this panel persists to.
    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    /// Presets currently listed in the panel.
    pub fn presets(&self) -> &[CameraPreset] {
        &self.presets
    }

    /// The highlighted preset row, if any.
    pub fn active_name(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// The highlighted built-in field preset row, if any.
    pub fn active_field_name(&self) -> Option<&str> {
        self.active_field.as_deref()
    }

    /// The highlighted built-in source preset row, if any.
    pub fn active_source_name(&self) -> Option<&str> {
        self.active_source.as_deref()
    }

    /// The preset waiting for a delete confirmation, if any.
    pub fn pending_delete(&self) -> Option<&str> {
        self.pending_delete.as_deref()
    }

    /// The non-blocking storage note, if browser/native storage failed.
    pub fn storage_note(&self) -> Option<&str> {
        self.storage_note.as_deref()
    }

    /// The current search text. Session-only: never persisted.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Replace the search text. Applying a preset keeps it.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    /// The active dimension/animated filter set. Session-only.
    pub fn filters(&self) -> PresetFilters {
        self.filters
    }

    /// Replace the filters. Applying a preset keeps them.
    pub fn set_filters(&mut self, filters: PresetFilters) {
        self.filters = filters;
    }

    /// Whether `group` is expanded in the panel.
    pub fn expanded(&self, group: PresetGroup) -> bool {
        self.expanded[group_index(group)]
    }

    /// Expand or collapse `group`, reporting a user layout change to the app.
    /// Re-setting the current value is not a change.
    pub fn set_expanded(&mut self, group: PresetGroup, expanded: bool) {
        let slot = &mut self.expanded[group_index(group)];
        if *slot != expanded {
            *slot = expanded;
            self.layout_changed = true;
        }
    }

    /// The persisted expansion slots, in [`PRESET_GROUPS`] order.
    pub fn expanded_state(&self) -> [bool; 4] {
        self.expanded
    }

    /// Apply startup expansion without reporting a layout edit, so restoring
    /// settings never dirties the scene.
    pub fn restore_expanded(&mut self, expanded: [bool; 4]) {
        self.expanded = expanded;
    }

    /// Drain the pending expansion-change report. `true` exactly once per
    /// user toggle, so the app can mark the settings dirty.
    pub fn take_layout_changed(&mut self) -> bool {
        std::mem::take(&mut self.layout_changed)
    }

    /// The typed name currently in the "Save current view" field.
    pub fn new_name(&self) -> &str {
        &self.new_name
    }

    /// Replace the typed name in the "Save current view" field.
    pub fn set_new_name(&mut self, name: impl Into<String>) {
        self.new_name = name.into();
    }

    /// The camera state applied by a `load` this frame, for the app to move
    /// the live view. Taken once per click.
    pub fn take_applied(&mut self) -> Option<Camera> {
        self.applied.take()
    }

    /// Drop the row highlight (e.g. after the camera was reset elsewhere).
    pub fn clear_active(&mut self) {
        self.active = None;
    }

    /// Persist the current camera state under `name` and list it immediately.
    /// Saving with an existing name replaces that entry (existing ruling);
    /// the saved preset becomes the highlighted row.
    pub fn save_current(&mut self, name: &str, camera: &Camera) -> io::Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Err(error) = save_preset(&self.dir, name, camera) {
                warn!("preset save failed: {error}");
                self.storage_note = Some(format!("Camera preset could not be saved: {error}"));
                return Err(error);
            }
            self.storage_note = None;
        }
        self.presets.retain(|p| p.name != name);
        self.presets.push(CameraPreset {
            name: name.to_owned(),
            position: camera.position,
            scale: camera.scale,
        });
        #[cfg(target_arch = "wasm32")]
        self.persist_wasm();
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.storage_note = None;
        }
        self.active = Some(name.to_owned());
        debug!("preset saved: {name}");
        Ok(())
    }

    /// Apply the named preset to the camera and highlight its row. Returns
    /// false — leaving the camera and highlight untouched — if no such preset
    /// is listed.
    pub fn load(&mut self, name: &str, camera: &mut Camera) -> bool {
        match self.presets.iter().find(|p| p.name == name) {
            Some(p) => {
                camera.position = p.position;
                camera.scale = p.scale;
                self.active = Some(name.to_owned());
                self.applied = Some(Camera {
                    position: p.position,
                    scale: p.scale,
                });
                debug!("preset loaded: {name}");
                true
            }
            None => {
                warn!("preset load failed: no preset named {name}");
                false
            }
        }
    }

    /// Ask for confirmation before removing the named preset.
    pub fn request_delete(&mut self, name: &str) {
        self.pending_delete = Some(name.to_owned());
    }

    /// Drop the pending delete request (the user cancelled).
    pub fn cancel_delete(&mut self) {
        self.pending_delete = None;
    }

    /// Remove the named preset from the panel list and from disk. Clears both
    /// the pending request and the highlight when they name this preset.
    pub fn delete(&mut self, name: &str) -> io::Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Err(error) = delete_preset(&self.dir, name) {
                warn!("preset delete failed: {error}");
                self.storage_note = Some(format!("Camera preset could not be deleted: {error}"));
                return Err(error);
            }
            self.storage_note = None;
        }
        self.presets.retain(|p| p.name != name);
        if self.active.as_deref() == Some(name) {
            self.active = None;
        }
        if self.pending_delete.as_deref() == Some(name) {
            self.pending_delete = None;
        }
        #[cfg(target_arch = "wasm32")]
        self.persist_wasm();
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.storage_note = None;
        }
        debug!("preset deleted: {name}");
        Ok(())
    }

    /// Load a built-in field preset into the scene state: its formula and the
    /// matching 2D/3D mode. Returns false — leaving the state untouched — when
    /// no preset carries that name. Camera preset rows are unaffected.
    pub fn load_field(&mut self, name: &str, state: &mut State) -> bool {
        match presets().iter().find(|preset| preset.name == name) {
            Some(preset) => {
                // A field preset is always a vector formula: return to Formula
                // before the vector projection/formula setters so the saved
                // source scene is preserved but no longer sampled.
                state.set_field_mode(FieldMode::Formula);
                // A field preset is always a vector field: return there before
                // the vector projection/formula setters so a scalar kind can
                // never reinterpret the preset text as a scalar draft.
                state.set_plot_kind(PlotKind::VectorField);
                state.set_mode(preset.dimension == 3);
                state.set_formula(preset.formula.to_string());
                self.active_field = Some(name.to_owned());
                self.active_source = None;
                debug!("field preset loaded: {name}");
                true
            }
            None => {
                warn!("field preset load failed: no preset named {name}");
                false
            }
        }
    }

    /// Load one built-in source preset through the ordinary source-scene
    /// setters. Returns false — leaving the state and highlight untouched —
    /// when no preset carries that name. Camera preset rows are unaffected.
    pub fn load_source_field(&mut self, name: &str, state: &mut State) -> bool {
        match source_preset(name) {
            Some(preset) => {
                if !crate::source_presets::load_source_preset(preset, state) {
                    warn!("source preset load failed: {name} did not validate");
                    return false;
                }
                self.active_source = Some(name.to_owned());
                self.active_field = None;
                debug!("source preset loaded: {name}");
                true
            }
            None => {
                warn!("source preset load failed: no preset named {name}");
                false
            }
        }
    }

    /// Draw one library row: its label keeps the formula annotations, its
    /// summary follows inline, and a click applies through the existing
    /// formula/source loader. Returns true when a source entry loaded, so the
    /// caller can frame it. Expansion/search state is never touched here.
    fn show_entry(&mut self, ui: &mut egui::Ui, entry: &PresetEntry, state: &mut State) -> bool {
        let selected = match entry.group {
            PresetGroup::Classic | PresetGroup::Physics => presets()
                .iter()
                .find(|preset| preset.name == entry.name)
                .is_some_and(|preset| {
                    self.active_field.as_deref() == Some(entry.name)
                        && state.formula == preset.formula
                        && state.mode_3d == (preset.dimension == 3)
                }),
            PresetGroup::Sources | PresetGroup::Gallery => {
                state.field_mode == FieldMode::Sources
                    && self.active_source.as_deref() == Some(entry.name)
            }
        };
        let mut source_loaded = false;
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(selected, entry_label(entry))
                .on_hover_text(if entry.summary.is_empty() {
                    entry.filters.dimension.map_or_else(
                        || format!("Load the {} preset", entry.name),
                        |dimension| format!("Load the {dimension}D {} preset", entry.name),
                    )
                } else {
                    entry.summary.to_owned()
                })
                .clicked()
            {
                match entry.group {
                    PresetGroup::Classic | PresetGroup::Physics => {
                        self.load_field(entry.name, state);
                    }
                    PresetGroup::Sources | PresetGroup::Gallery => {
                        source_loaded = self.load_source_field(entry.name, state);
                    }
                }
            }
            if !entry.summary.is_empty() {
                ui.weak(entry.summary);
            }
        });
        source_loaded
    }

    /// Draw the panel. Returns the camera command a `Reset view` / `Fit
    /// domain` button asked for this frame (the app applies it through the
    /// view-command seam).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        camera: &mut Camera,
        state: &mut State,
    ) -> Option<ViewCommand> {
        self.show_compact(ui, camera, state, false)
    }

    pub fn show_compact(
        &mut self,
        ui: &mut egui::Ui,
        camera: &mut Camera,
        state: &mut State,
        compact: bool,
    ) -> Option<ViewCommand> {
        if compact {
            ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
            ui.spacing_mut().item_spacing.y = crate::ui::layout::COMPACT_ITEM_SPACING;
        }
        self.show_inner(ui, camera, state)
    }

    fn show_inner(
        &mut self,
        ui: &mut egui::Ui,
        camera: &mut Camera,
        state: &mut State,
    ) -> Option<ViewCommand> {
        ui.heading("Presets");

        // Camera actions (ticket 18): clickable twins of `R` and `F`, kept
        // next to the presets so the view controls live in one place.
        let mut requested = None;
        ui.horizontal_wrapped(|ui| {
            for (label, command) in CAMERA_BUTTONS {
                let tooltip = match command {
                    ViewCommand::Reset => "Restore the default view (R)",
                    ViewCommand::Fit => "Frame the whole domain (F)",
                    _ => "",
                };
                if ui.button(*label).on_hover_text(tooltip).clicked() {
                    requested = Some(*command);
                }
            }
        });

        // Search and filters narrow the merged library for this frame; the
        // query is session-only while the expansion below persists.
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search presets")
                .desired_width(200.0),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Dimension");
            ui.selectable_value(&mut self.filters.dimension, None, "All");
            ui.selectable_value(&mut self.filters.dimension, Some(2), "2D");
            ui.selectable_value(&mut self.filters.dimension, Some(3), "3D");
            ui.checkbox(&mut self.filters.animated, "Animated");
        });

        let matches: Vec<PresetEntry> = self
            .library
            .search_filtered(&self.query, self.filters)
            .into_iter()
            .cloned()
            .collect();
        // Keep the old quick-start rows above the full catalogue. They follow
        // the same search/filter and load through the same row method, so the
        // panel still opens with the familiar three-row shortlist.
        let start_here: Vec<&PresetEntry> = START_HERE
            .iter()
            .filter_map(|name| matches.iter().find(|entry| entry.name == *name))
            .collect();
        if !start_here.is_empty() {
            ui.weak("Start here");
            for entry in start_here {
                if self.show_entry(ui, entry, state) {
                    requested = Some(ViewCommand::Fit);
                }
            }
            ui.add_space(4.0);
        }
        if matches.is_empty() {
            ui.weak("No presets match");
        } else {
            for (index, group) in PRESET_GROUPS.into_iter().enumerate() {
                let entries: Vec<&PresetEntry> = matches
                    .iter()
                    .filter(|entry| entry.group == group)
                    .collect();
                if entries.is_empty() {
                    continue;
                }
                let title = format!("{} ({})", group_title(group), entries.len());
                let response = egui::CollapsingHeader::new(title)
                    .id_salt(("preset_group", index))
                    .open(Some(self.expanded[index]))
                    .show(ui, |ui| {
                        for entry in entries {
                            if self.show_entry(ui, entry, state) {
                                requested = Some(ViewCommand::Fit);
                            }
                        }
                    });
                // A header click is a layout change, never a preset load.
                if response.header_response.clicked() {
                    self.expanded[index] = !self.expanded[index];
                    self.layout_changed = true;
                }
            }
        }

        ui.separator();
        ui.weak("Saved views");
        if let Some(note) = &self.storage_note {
            ui.weak(note).on_hover_text(
                "Saving still works for this session; the browser refused persistent storage.",
            );
        }
        if self.presets.is_empty() {
            ui.weak("No saved views yet.");
        }

        // Rows for the current list. Names are cloned so the list can be
        // mutated inside the loop.
        let names: Vec<String> = self.presets.iter().map(|p| p.name.clone()).collect();
        for name in &names {
            let is_active = self.active.as_deref() == Some(name.as_str());
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(is_active, name)
                    .on_hover_text("Apply this saved view")
                    .clicked()
                {
                    self.load(name, camera);
                }
                if is_active {
                    ui.weak("active");
                }
                if ui
                    .button("Delete")
                    .on_hover_text("Delete this preset (asks for confirmation)")
                    .clicked()
                {
                    self.request_delete(name);
                }
            });
            if self.pending_delete.as_deref() == Some(name.as_str()) {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(230, 70, 70),
                        format!("Delete \"{name}\"?"),
                    );
                    if ui.button("Delete").clicked()
                        && let Err(e) = self.delete(name)
                    {
                        warn!("preset delete failed: {e}");
                    }
                    if ui.button("Cancel").clicked() {
                        self.cancel_delete();
                    }
                });
            }
        }

        // Save the current camera as a preset. Duplicate names replace the
        // existing entry (existing ruling), so the name field is not claimed
        // to be unique.
        ui.separator();
        ui.label("Save current view");
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("Preset name")
                    .desired_width(140.0),
            );
            let name = self.new_name.clone();
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("Save current view"))
                .on_hover_text(
                    "Save the current camera under this name (replaces an existing name)",
                )
                .clicked()
            {
                if self.save_current(&name, camera).is_ok() {
                    self.new_name.clear();
                } else {
                    warn!("preset save failed");
                }
            }
        });

        requested
    }
}
