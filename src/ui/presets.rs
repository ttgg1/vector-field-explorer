use eframe::egui;
use log::{debug, warn};
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::io;
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use crate::export::{delete_preset, load_presets, save_preset};
#[cfg(target_arch = "wasm32")]
use crate::export::{load_wasm_presets, save_wasm_presets};
use crate::export::Preset;
use crate::presets::{PresetCategory, START_HERE, presets, presets_in};
use crate::render::Camera;
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
    presets: Vec<Preset>,
    /// Pending name for the "Save current view" name field.
    new_name: String,
    /// The preset last saved or applied; drives the row highlight.
    active: Option<String>,
    /// The built-in field preset last clicked, so duplicate formulas can
    /// highlight only the row the user picked.
    active_field: Option<String>,
    /// Preset awaiting the delete confirmation.
    pending_delete: Option<String>,
    /// A camera loaded this frame, for the app to hand to the live view. Taken
    /// once so one click applies one camera.
    applied: Option<Camera>,
    /// One non-blocking note when browser storage cannot hold the list.
    storage_note: Option<String>,
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
                warn!("could not create preset directory {}: {error}", dir.display());
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

    fn from_parts(dir: PathBuf, presets: Vec<Preset>, storage_note: Option<String>) -> Self {
        Self {
            dir,
            presets,
            new_name: String::new(),
            active: None,
            active_field: None,
            pending_delete: None,
            applied: None,
            storage_note,
        }
    }

    /// Panel pointed at an explicit native directory (used by tests).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_with_dir(dir: PathBuf) -> io::Result<Self> {
        if let Err(error) = fs::create_dir_all(&dir) {
            warn!("could not create preset directory {}: {error}", dir.display());
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
    pub fn presets(&self) -> &[Preset] {
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

    /// The preset waiting for a delete confirmation, if any.
    pub fn pending_delete(&self) -> Option<&str> {
        self.pending_delete.as_deref()
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
        save_preset(&self.dir, name, camera)?;
        self.presets.retain(|p| p.name != name);
        self.presets.push(Preset {
            name: name.to_owned(),
            position: camera.position,
            scale: camera.scale,
        });
        #[cfg(target_arch = "wasm32")]
        self.persist_wasm();
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
                self.applied = Some(Camera { position: p.position, scale: p.scale });
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
        delete_preset(&self.dir, name)?;
        self.presets.retain(|p| p.name != name);
        if self.active.as_deref() == Some(name) {
            self.active = None;
        }
        if self.pending_delete.as_deref() == Some(name) {
            self.pending_delete = None;
        }
        #[cfg(target_arch = "wasm32")]
        self.persist_wasm();
        debug!("preset deleted: {name}");
        Ok(())
    }

    /// Load a built-in field preset into the scene state: its formula and the
    /// matching 2D/3D mode. Returns false — leaving the state untouched — when
    /// no preset carries that name. Camera preset rows are unaffected.
    pub fn load_field(&mut self, name: &str, state: &mut State) -> bool {
        match presets().iter().find(|preset| preset.name == name) {
            Some(preset) => {
                state.set_mode(preset.dimension == 3);
                state.set_formula(preset.formula.to_string());
                self.active_field = Some(name.to_owned());
                debug!("field preset loaded: {name}");
                true
            }
            None => {
                warn!("field preset load failed: no preset named {name}");
                false
            }
        }
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
        ui.heading("Presets");

        // Camera actions (ticket 18): clickable twins of `R` and `F`, kept
        // next to the presets so the view controls live in one place.
        let mut requested = None;
        ui.horizontal(|ui| {
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

        // Built-in field presets, grouped by category. The row highlight needs
        // the clicked name (`active_field`) as well as the live formula and
        // dimension, so duplicate formulas highlight one row and formula-bar
        // edits clear the highlight.
        ui.weak("Field presets");
        ui.weak("Start here");
        for name in START_HERE {
            let preset = presets().iter().find(|preset| *preset.name == *name);
            let (label, selected) = match preset {
                Some(preset) => {
                    let selected = self.active_field.as_deref() == Some(name)
                        && state.formula == preset.formula
                        && state.mode_3d == (preset.dimension == 3);
                    let label = if preset.dimension == 3 {
                        format!("{} (3D)", preset.name)
                    } else {
                        preset.name.to_string()
                    };
                    (label, selected)
                }
                None => (name.to_string(), false),
            };
            if ui
                .selectable_label(selected, label)
                .on_hover_text(format!("Load the {name} preset"))
                .clicked()
            {
                self.load_field(name, state);
            }
        }
        for category in PresetCategory::ALL {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(category.title()).strong())
                .on_hover_text(category.description());
            for preset in presets_in(category) {
                let selected = self.active_field.as_deref() == Some(preset.name)
                    && state.formula == preset.formula
                    && state.mode_3d == (preset.dimension == 3);
                let label = if preset.dimension == 3 {
                    format!("{} (3D)", preset.name)
                } else {
                    preset.name.to_string()
                };
                if ui
                    .selectable_label(selected, label)
                    .on_hover_text(format!(
                        "Load the {} preset ({}D field)",
                        preset.name, preset.dimension
                    ))
                    .clicked()
                {
                    self.load_field(preset.name, state);
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
            ui.horizontal(|ui| {
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
                ui.horizontal(|ui| {
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
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("Preset name")
                    .desired_width(140.0),
            );
            let name = self.new_name.clone();
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("Save current view"))
                .on_hover_text("Save the current camera under this name (replaces an existing name)")
                .clicked()
            {
                if let Err(e) = self.save_current(&name, camera) {
                    warn!("preset save failed: {e}");
                }
                self.new_name.clear();
            }
        });

        requested
    }
}
