//! The Interaction panel (issue #88 slice A2b).
//!
//! Exposes the pure A1 interaction model: one row per remappable action with a
//! rebind capture mode and an atomic conflict replace, the camera-feel
//! multipliers and invert flags, and the built-in/user profile library with
//! portable import/export. Every edit commits directly to the live
//! [`InteractionProfile`], so the existing settings save persists it; panel
//! drafts and messages are session-only. Input routing is slice A3 and is not
//! touched here.

use eframe::egui;

use crate::interaction::{
    BindError, Bindings, CameraFeel, ChordModifiers, DEFAULT_PROFILE_NAME, InteractionAction,
    InteractionProfile, KeyChord, MAX_CAMERA_MULTIPLIER, MIN_CAMERA_MULTIPLIER, ProfileDocument,
    ProfileLibrary,
};

/// Stable id for the profile-name field. Capture must ignore keys while this
/// field owns the keyboard, so typing a name never binds a chord.
const PROFILE_NAME_FIELD_ID: &str = "interaction_panel.profile_name";

/// Stable id for the import field, guarded the same way as the name field.
const IMPORT_FIELD_ID: &str = "interaction_panel.import_text";

/// A captured chord that another action already owns, waiting for Replace or
/// Cancel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingConflict {
    /// The action the user is rebinding.
    pub action: InteractionAction,
    /// The chord the user captured.
    pub chord: KeyChord,
    /// The action that currently owns the chord.
    pub owner: InteractionAction,
}

/// Panel-local state: the capture target, drafts, one message slot and the
/// delete confirmation. Never persisted.
#[derive(Debug, Clone, Default)]
pub struct InteractionPanel {
    /// The action waiting for the next key press, if any.
    pub capturing: Option<InteractionAction>,
    /// The save-current name draft.
    pub profile_name: String,
    /// The import draft.
    pub import_text: String,
    /// The cached portable export, refreshed by Export and after every
    /// successful library mutation.
    pub export_text: String,
    /// The last panel result or rejection. A later result replaces it.
    pub message: Option<String>,
    /// The name awaiting the second delete click.
    pub pending_delete: Option<String>,
    /// The unresolved captured-chord conflict, if any.
    pub conflict: Option<PendingConflict>,
}

impl InteractionPanel {
    /// Draw the panel body. `compact` raises the row height to the shared
    /// finger-sized floor used by the compact bottom sheet.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        live: &mut InteractionProfile,
        profiles: &mut ProfileLibrary,
        compact: bool,
    ) {
        if compact {
            ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
            ui.spacing_mut().item_spacing.y = crate::ui::layout::COMPACT_ITEM_SPACING;
        }

        self.handle_capture(ui, live);

        ui.heading("Interaction");
        ui.label("Bindings and camera feel apply to the live profile; input routing follows in a later slice.");
        if let Some(message) = self.message.as_ref() {
            ui.label(message);
        }

        self.bindings_section(ui, live);
        self.camera_feel_section(ui, live);
        self.profiles_section(ui, live, profiles);
    }

    /// Enter capture mode for `action`. A later key press is assigned through
    /// [`Bindings::assign`], which never changes anything on a refusal.
    pub fn begin_capture(&mut self, action: InteractionAction) {
        self.capturing = Some(action);
        self.conflict = None;
        self.message = Some(format!(
            "Press a key for {}; Escape cancels",
            action.label()
        ));
    }

    /// Drop an armed capture and any pending conflict because the panel is no
    /// longer drawn. A hidden panel must never keep a chord latched: otherwise
    /// the next key press after reopening would bind silently.
    pub fn cancel_capture(&mut self) {
        if self.capturing.is_some() || self.conflict.is_some() {
            self.capturing = None;
            self.conflict = None;
            self.message = Some("Cancelled the rebind".to_owned());
        }
    }

    /// Bind the captured `chord` to the capturing action. A conflict is held
    /// for an explicit Replace or Cancel; every other refusal is reported and
    /// changes nothing.
    pub fn apply_capture(&mut self, live: &mut InteractionProfile, chord: KeyChord) {
        let Some(action) = self.capturing else {
            return;
        };
        match live.bindings.assign(action, chord) {
            Ok(()) => {
                self.capturing = None;
                self.conflict = None;
                self.message = Some(format!("Bound {} to {}", action.label(), chord.label()));
            }
            Err(BindError::Conflict(owner)) => {
                self.capturing = None;
                self.conflict = Some(PendingConflict {
                    action,
                    chord,
                    owner,
                });
                self.message = Some(format!(
                    "{} is already used by {}. Choose Replace to swap both bindings, or Cancel.",
                    chord.label(),
                    owner.label()
                ));
            }
            Err(error) => {
                self.capturing = None;
                self.conflict = None;
                self.message = Some(bind_error_text(error, chord));
            }
        }
    }

    /// Apply the pending conflict's atomic swap. Both actions stay bound and
    /// no duplicate chord remains.
    pub fn resolve_conflict(&mut self, live: &mut InteractionProfile) {
        let Some(conflict) = self.conflict else {
            return;
        };
        match live.bindings.replace(conflict.action, conflict.chord) {
            Ok(()) => {
                self.conflict = None;
                self.message = Some(format!(
                    "Bound {} to {} and moved the previous chord to {}",
                    conflict.action.label(),
                    conflict.chord.label(),
                    conflict.owner.label()
                ));
            }
            Err(error) => {
                self.conflict = None;
                self.message = Some(bind_error_text(error, conflict.chord));
            }
        }
    }

    /// Cancel the pending conflict without touching either binding.
    pub fn cancel_conflict(&mut self) {
        self.conflict = None;
        self.message = Some("Cancelled the rebind".to_owned());
    }

    /// Restore one action's documented default chord, swapping atomically when
    /// another action currently owns it.
    pub fn reset_action(&mut self, live: &mut InteractionProfile, action: InteractionAction) {
        let Some(default) = Bindings::default_bindings().chord(action) else {
            self.message = Some(format!(
                "{} is reserved and cannot be reset",
                action.label()
            ));
            return;
        };
        match live.bindings.replace(action, default) {
            Ok(()) => {
                self.message = Some(format!("Reset {} to {}", action.label(), default.label()));
            }
            Err(error) => {
                self.message = Some(bind_error_text(error, default));
            }
        }
    }

    /// Commit a camera-feel edit. Multipliers clamp to the documented range and
    /// non-finite values sanitize to the default, so a direct write can never
    /// store an invalid feel.
    pub fn commit_camera_feel(&mut self, live: &mut InteractionProfile, feel: CameraFeel) {
        live.camera_feel = feel.sanitized();
    }

    /// Restore [`CameraFeel::default`] without touching bindings.
    pub fn reset_camera_feel(&mut self, live: &mut InteractionProfile) {
        live.camera_feel = CameraFeel::default();
        self.message = Some("Reset camera feel to defaults".to_owned());
    }

    /// Replace the live bindings and feel with `profile`'s. The live record's
    /// name is untouched, and no other app state is read or written.
    pub fn apply_profile(&mut self, live: &mut InteractionProfile, profile: &InteractionProfile) {
        live.bindings = profile.bindings.clone();
        live.camera_feel = profile.camera_feel;
        self.message = Some(format!("Applied {}", profile.name));
    }

    /// Capture the live bindings and feel under the typed name. Empty and
    /// built-in names are rejected; a same-named user profile is replaced in
    /// place, and library validation/cap errors are reported verbatim.
    pub fn save_current(&mut self, live: &InteractionProfile, profiles: &mut ProfileLibrary) {
        let name = self.profile_name.trim().to_owned();
        if name.is_empty() {
            self.message = Some("enter a profile name before saving".to_owned());
            return;
        }
        let replaces = profiles.user().iter().any(|profile| profile.name == name);
        let profile = InteractionProfile {
            name: name.clone(),
            bindings: live.bindings.clone(),
            camera_feel: live.camera_feel,
        };
        match profiles.upsert(profile) {
            Ok(()) => {
                self.profile_name.clear();
                self.pending_delete = None;
                self.sync_export(profiles);
                let action = if replaces { "Replaced" } else { "Saved" };
                self.message = Some(format!("{action} profile {name}"));
            }
            Err(error) => self.message = Some(error),
        }
    }

    /// Arm the first delete click; the second click removes the user profile.
    pub fn delete_profile(&mut self, profiles: &mut ProfileLibrary, name: &str) {
        if name == DEFAULT_PROFILE_NAME {
            self.message = Some(format!(
                "{name:?} is a built-in profile and cannot be deleted"
            ));
            return;
        }
        if self.pending_delete.as_deref() != Some(name) {
            self.pending_delete = Some(name.to_owned());
            self.message = Some(format!("Click Delete again to remove {name}"));
            return;
        }

        self.pending_delete = None;
        if profiles.remove(name) {
            self.sync_export(profiles);
            self.message = Some(format!("Deleted profile {name}"));
        } else {
            self.message = Some(format!("profile {name} was not found"));
        }
    }

    /// Import the draft atomically and report how many new user profiles it
    /// added. Replacements alone say so honestly; an empty document says
    /// `Nothing to import`.
    pub fn import_profiles(&mut self, profiles: &mut ProfileLibrary) {
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
                self.message = Some(message);
            }
            Err(error) => self.message = Some(error),
        }
    }

    /// Recompute the cached portable export from the user library.
    pub fn sync_export(&mut self, profiles: &ProfileLibrary) {
        self.export_text = if profiles.is_empty() {
            String::new()
        } else {
            profiles.export_text()
        };
    }

    fn handle_capture(&mut self, ui: &egui::Ui, live: &mut InteractionProfile) {
        if self.capturing.is_none() {
            return;
        }
        // The panel's own text fields stay usable during capture: while one
        // owns the keyboard, a typed key belongs to the field, not the chord.
        if text_field_has_focus(ui) {
            return;
        }
        let captured = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if !is_modifier_key(*key) => Some((*key, *modifiers)),
                _ => None,
            })
        });
        let Some((key, modifiers)) = captured else {
            return;
        };
        if key == egui::Key::Escape {
            self.capturing = None;
            self.conflict = None;
            self.message = Some("Cancelled the rebind".to_owned());
            return;
        }
        self.apply_capture(
            live,
            KeyChord {
                key,
                modifiers: ChordModifiers {
                    command: modifiers.command,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                },
            },
        );
    }

    fn bindings_section(&mut self, ui: &mut egui::Ui, live: &mut InteractionProfile) {
        ui.separator();
        ui.label(egui::RichText::new("Bindings").strong());

        for action in InteractionAction::REMAPPABLE {
            let chord = live.bindings.chord(action);
            ui.horizontal_wrapped(|ui| {
                ui.label(action.label());
                ui.weak(
                    chord
                        .map(|chord| chord.label())
                        .unwrap_or_else(|| "(unbound)".to_owned()),
                );
                let idle = self.capturing.is_none();
                if ui
                    .add_enabled(idle, egui::Button::new("Rebind"))
                    .on_hover_text("Capture the next key press for this action")
                    .clicked()
                {
                    self.begin_capture(action);
                }
                if ui
                    .add_enabled(idle, egui::Button::new("Reset action"))
                    .on_hover_text("Restore this action's documented default chord")
                    .clicked()
                {
                    self.reset_action(live, action);
                }
            });
        }

        if let Some(conflict) = self.conflict {
            ui.group(|ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "{} is already used by {}.",
                        conflict.chord.label(),
                        conflict.owner.label()
                    ),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button("Replace")
                        .on_hover_text("Swap the two chords so both actions stay bound")
                        .clicked()
                    {
                        self.resolve_conflict(live);
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Leave both bindings untouched")
                        .clicked()
                    {
                        self.cancel_conflict();
                    }
                });
            });
        }

        for action in [
            InteractionAction::Dismiss,
            InteractionAction::Undo,
            InteractionAction::Redo,
        ] {
            ui.horizontal_wrapped(|ui| {
                ui.label(action.label());
                ui.weak(
                    reserved_chord(action)
                        .map(|chord| chord.label())
                        .unwrap_or_default(),
                );
                ui.weak("(reserved)");
            });
        }
    }

    fn camera_feel_section(&mut self, ui: &mut egui::Ui, live: &mut InteractionProfile) {
        ui.separator();
        ui.label(egui::RichText::new("Camera feel").strong());

        let mut feel = live.camera_feel;
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Orbit sensitivity");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut feel.orbit_sensitivity)
                        .range(MIN_CAMERA_MULTIPLIER..=MAX_CAMERA_MULTIPLIER)
                        .speed(0.01),
                )
                .changed();
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Zoom speed");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut feel.zoom_speed)
                        .range(MIN_CAMERA_MULTIPLIER..=MAX_CAMERA_MULTIPLIER)
                        .speed(0.01),
                )
                .changed();
        });
        changed |= ui.checkbox(&mut feel.invert_yaw, "Invert yaw").changed();
        changed |= ui
            .checkbox(&mut feel.invert_pitch, "Invert pitch")
            .changed();
        changed |= ui.checkbox(&mut feel.invert_zoom, "Invert zoom").changed();
        if ui
            .button("Reset camera feel")
            .on_hover_text("Restore the default multipliers and invert flags")
            .clicked()
        {
            self.reset_camera_feel(live);
        }
        if changed {
            self.commit_camera_feel(live, feel);
        }
    }

    fn profiles_section(
        &mut self,
        ui: &mut egui::Ui,
        live: &mut InteractionProfile,
        profiles: &mut ProfileLibrary,
    ) {
        ui.separator();
        ui.label(egui::RichText::new("Profiles").strong());

        let all = profiles.all();
        for profile in &all {
            ui.horizontal_wrapped(|ui| {
                ui.label(&profile.name);
                if ui
                    .button("Apply")
                    .on_hover_text("Apply this interaction profile")
                    .clicked()
                {
                    self.apply_profile(live, profile);
                }
                if profile.name != DEFAULT_PROFILE_NAME {
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

        if ui
            .button("Restore defaults")
            .on_hover_text("Install the documented default bindings and camera feel")
            .clicked()
        {
            live.bindings = Bindings::default_bindings();
            live.camera_feel = CameraFeel::default();
            self.message = Some("Restored default bindings and camera feel".to_owned());
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            ui.add(
                egui::TextEdit::singleline(&mut self.profile_name)
                    .id(egui::Id::unique(PROFILE_NAME_FIELD_ID))
                    .hint_text("Profile name")
                    .desired_width(180.0),
            );
        });
        if ui.button("Save current interaction").clicked() {
            self.save_current(live, profiles);
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button("Export text").clicked() {
                self.sync_export(profiles);
            }
            let can_copy = !self.export_text.is_empty();
            if ui
                .add_enabled(can_copy, egui::Button::new("Copy"))
                .on_hover_text("Copy the interaction profile document")
                .clicked()
            {
                let text = self.export_text.clone();
                ui.ctx().copy_text(text);
                self.message = Some("Copied profile text".to_owned());
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
                .id(egui::Id::unique(IMPORT_FIELD_ID))
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Paste an interaction profile document"),
        );
        if ui.button("Apply text").clicked() {
            self.import_profiles(profiles);
        }
    }
}

fn is_modifier_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::ShiftLeft
            | egui::Key::ShiftRight
            | egui::Key::ControlLeft
            | egui::Key::ControlRight
            | egui::Key::AltLeft
            | egui::Key::AltRight
            | egui::Key::SuperLeft
            | egui::Key::SuperRight
    )
}

/// Whether one of the panel's own text fields currently owns the keyboard.
fn text_field_has_focus(ui: &egui::Ui) -> bool {
    ui.memory(|memory| memory.focused()).is_some_and(|focused| {
        focused == egui::Id::unique(PROFILE_NAME_FIELD_ID)
            || focused == egui::Id::unique(IMPORT_FIELD_ID)
    })
}

/// The fixed, read-only chord for a reserved action.
///
/// Reserved chords are hard-coded here because `src/interaction.rs` is out of
/// scope for this slice; revisit if A1's reserved set changes.
fn reserved_chord(action: InteractionAction) -> Option<KeyChord> {
    let command = ChordModifiers {
        command: true,
        shift: false,
        alt: false,
    };
    Some(match action {
        InteractionAction::Dismiss => KeyChord {
            key: egui::Key::Escape,
            modifiers: ChordModifiers::default(),
        },
        InteractionAction::Undo => KeyChord {
            key: egui::Key::Z,
            modifiers: command,
        },
        InteractionAction::Redo => KeyChord {
            key: egui::Key::Z,
            modifiers: ChordModifiers {
                shift: true,
                ..command
            },
        },
        _ => return None,
    })
}

fn bind_error_text(error: BindError, chord: KeyChord) -> String {
    match error {
        BindError::Reserved => format!("{} is reserved and cannot be bound", chord.label()),
        BindError::BrowserReserved => format!(
            "{} is owned by the browser and cannot be bound",
            chord.label()
        ),
        BindError::Conflict(owner) => {
            format!("{} is already used by {}", chord.label(), owner.label())
        }
        BindError::Unknown => format!("{} cannot be bound", chord.label()),
    }
}
