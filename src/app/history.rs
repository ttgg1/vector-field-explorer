//! Issue #86 slice A2: the app-level edit-history wiring.
//!
//! [`capture_document`] and [`apply_document`] are the one conversion between
//! the live app and the pure [`EditSnapshot`] value from slice A1. The frame
//! driver records one entry per pointer gesture and direct entries for edits
//! made by menus, shortcuts, presets and loads. Runtime state (clock, particles,
//! fly pose, layout, caches) is never captured, so undo cannot rewind time or
//! replay file effects.

use super::*;

use crate::appearance_profile::AppearanceProfile;
use crate::edit_history::{EditScope, EditSnapshot};
use crate::scene_document::SceneCamera;
use crate::ui::menu;

/// Capture the editable document from one live state and camera pose.
///
/// Both colour copies (`ViewRecord` and the appearance profile) come from the
/// same `State` in this single call, as the accepted contract requires. The 2D
/// camera record mirrors the orbit target and ortho zoom, exactly like the
/// saved-scene handoff.
pub(crate) fn capture_document(state: &State, orbit: &OrbitCamera) -> EditSnapshot {
    EditSnapshot::new(
        ViewRecord::capture(state),
        state.parameters.clone(),
        SceneCamera {
            position: [orbit.target.x, orbit.target.y],
            scale: orbit.ortho_zoom,
            yaw: orbit.yaw,
            pitch: orbit.pitch,
            distance: orbit.distance,
            target: orbit.target.to_array(),
            fov_y: orbit.fov_y,
            ortho_zoom: orbit.ortho_zoom,
        },
        AppearanceProfile::capture("", state),
    )
}

/// Apply a snapshot to the live document: view first, then parameters, then the
/// camera with the live saved-scene clamps, then the appearance profile from the
/// same source. No file IO, preset writes or profile-library mutation. `_fly` is
/// part of the seam signature but deliberately untouched: free flight is runtime
/// state (see [`apply_camera`]).
pub(crate) fn apply_document(
    snapshot: &EditSnapshot,
    state: &mut State,
    controls: &mut Controls,
    orbit: &mut OrbitCamera,
    _fly: &mut Option<FlyCamera>,
) {
    // The view restore validates parameter-dependent entries (the custom
    // density filter) against the committed parameter set, so install the
    // snapshot's parameters first, mirroring `AppSettings::restore`. Put the
    // live set back before the explicit commit below: it then observes a
    // change and performs the field re-parse/rebuild that the direct
    // assignment alone would skip.
    let live_parameters = std::mem::replace(&mut state.parameters, snapshot.parameters.clone());
    snapshot.view.restore(state, controls);
    state.parameters = live_parameters;
    if let Err(error) = state.commit_parameters(snapshot.parameters.clone()) {
        // Snapshots are captured from a validated committed state; a failure
        // here is an internal construction bug, never a user edit.
        log::warn!("edit-history parameters rejected: {error}");
    }
    apply_camera(&snapshot.camera, orbit);
    // The snapshot name is a sentinel, not style data: capture uses `""` and
    // `AppearanceProfile::apply` validates names, so give the style payload a
    // valid sentinel before applying it. Every style field still comes from
    // this one snapshot.
    let mut appearance = snapshot.appearance.clone();
    appearance.name = crate::appearance_profile::DEFAULT_PROFILE_NAME.to_owned();
    if let Err(error) = appearance.apply(state) {
        log::warn!("edit-history appearance rejected: {error}");
    }
}

/// Apply the saved camera pose with the same live clamps `load_scene` uses.
///
/// The fly pose is runtime state and deliberately untouched: undo while flying
/// restores the document without moving or exiting free flight (the accepted
/// `#86` contract). `Camera`/`All` resets clear fly at their own reset seam.
fn apply_camera(camera: &SceneCamera, orbit: &mut OrbitCamera) {
    orbit.yaw = camera.yaw;
    // Deliberately mirrors the live `OrbitCamera::orbit` pitch clamp.
    orbit.pitch = camera.pitch.clamp(-1.55, 1.55);
    orbit.distance = camera
        .distance
        .clamp(OrbitCamera::MIN_DISTANCE, OrbitCamera::MAX_DISTANCE);
    orbit.target = Vec3::from_array(camera.target);
    orbit.fov_y = camera
        .fov_y
        .clamp(1.0_f32.to_radians(), 179.0_f32.to_radians());
    orbit.ortho_zoom = camera
        .ortho_zoom
        .clamp(OrbitCamera::MIN_ORTHO_ZOOM, OrbitCamera::MAX_ORTHO_ZOOM);
}

impl VfApp {
    /// Per-frame driver, called once after the UI has run.
    ///
    /// A primary-button press opens one transaction; every frame while any
    /// button is down is suspended; the primary release commits at most one
    /// entry. A secondary press/release is never a gesture boundary. Outside a
    /// gesture a document change made by a menu, shortcut, preset or load is
    /// recorded directly. Idle frames, simulation ticks and save paths never
    /// record.
    pub(super) fn drive_history(&mut self, ctx: &egui::Context) {
        let (pressed, down, released) = ctx.input(|input| {
            (
                // Only the primary button delimits a document gesture. A
                // secondary press/release must neither commit nor split the
                // open transaction; `any_down` still suspends recording for
                // every button (e.g. a context-menu chord during a drag).
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.any_down(),
                input.pointer.button_released(egui::PointerButton::Primary),
            )
        });

        if pressed {
            let before = capture_document(&self.state, &self.orbit);
            self.history.begin(before);
        }
        if released {
            let after = capture_document(&self.state, &self.orbit);
            // Camera orbit/pan/zoom is navigation, not a document edit: a
            // camera-only gesture commits nothing and keeps its pose in the
            // baseline so a later edit does not rewind it.
            if self.last_committed.document_changed(&after) {
                let label = self.last_committed.describe_change(&after);
                self.history.commit_named(&after, label);
            } else {
                self.history.cancel();
            }
            self.last_committed = after;
            return;
        }
        if down {
            // One gesture is one entry: no direct recording between press and
            // release, even when a panel commits every frame.
            return;
        }
        if !pressed && self.history.is_recording() {
            // A release that never arrived (focus loss) must not keep a stale
            // transaction alive into the next gesture.
            self.history.cancel();
        }

        // Idle-frame cost: capturing clones a `ViewRecord`, `ParameterSet` and
        // `AppearanceProfile` and compares them on every frame. Accepted for
        // this slice because correctness comes first; the intended cheap path
        // is a dirty/fingerprint gate that skips the capture when nothing the
        // document owns has changed.
        let current = capture_document(&self.state, &self.orbit);
        if current != self.last_committed {
            // Camera navigation alone updates the baseline without recording:
            // the undo stack holds document edits, never orbits or zoom.
            if current.document_changed(&self.last_committed) {
                let label = self.last_committed.describe_change(&current);
                self.history
                    .record_named(self.last_committed.clone(), &current, label);
            }
            self.last_committed = current;
        }
    }

    /// Undo one document edit. Inert while a text field owns the keyboard, so
    /// typing protection matches the rest of the shortcut table. Runtime state
    /// (fly pose, clock, particles, layout) is untouched.
    pub fn undo_document(&mut self, ctx: &egui::Context) -> bool {
        self.move_history(ctx, false)
    }

    /// Redo one document edit, with the same text-field and runtime rules.
    pub fn redo_document(&mut self, ctx: &egui::Context) -> bool {
        self.move_history(ctx, true)
    }

    fn move_history(&mut self, ctx: &egui::Context, redo: bool) -> bool {
        if ctx.text_edit_focused() {
            return false;
        }
        // An undo/redo is not part of an open gesture.
        self.history.cancel();
        let current = capture_document(&self.state, &self.orbit);
        let next = if redo {
            self.history.redo(current)
        } else {
            self.history.undo(current)
        };
        let Some(next) = next else {
            return false;
        };
        self.apply_snapshot(&next);
        // Applying restores the document; refresh the baseline so the driver
        // does not record the undo itself as a new edit.
        self.last_committed = capture_document(&self.state, &self.orbit);
        true
    }

    fn apply_snapshot(&mut self, snapshot: &EditSnapshot) {
        // A snapshot replaces the whole document, possibly with content from a
        // different saved scene than the one currently bound. Unbind first so
        // the next idle sync cannot write the restored document under the old
        // scene's name and overwrite it. Re-binding needs an explicit load or
        // save; an ordinary edit is never attributed to a scene across undo.
        self.presets.clear_active_scene();
        apply_document(
            snapshot,
            &mut self.state,
            &mut self.controls,
            &mut self.orbit,
            &mut self.fly,
        );
        self.camera.position = [self.orbit.target.x, self.orbit.target.y];
        self.camera.scale = self.orbit.ortho_zoom;
        self.bar.text = self.state.active_source().to_string();
    }

    /// Record the current document as one named entry when it differs from the
    /// baseline. Used by explicit commands (such as CAS camera framing) whose
    /// only change is the camera pose, which the idle driver deliberately
    /// leaves out as navigation.
    pub(crate) fn record_explicit_edit(&mut self, label: &str) {
        let before = self.last_committed.clone();
        let after = capture_document(&self.state, &self.orbit);
        if after == before {
            return;
        }
        self.history.cancel();
        self.history.record_named(before, &after, label);
        self.last_committed = after;
    }

    /// The one seam behind every scoped reset menu action.
    ///
    /// `Color`, `Camera` and `All` change the document, so they are recorded as
    /// one named entry here. Recording directly (rather than waiting for the
    /// idle driver) is what keeps the explicit `Camera` reset in history even
    /// though ordinary orbit/zoom navigation is deliberately outside the undo
    /// stack. `Simulation` touches runtime state only and is undo-free by
    /// construction: the snapshot cannot see it.
    pub fn reset_scope(&mut self, scope: EditScope) {
        let before = self.last_committed.clone();
        match scope {
            EditScope::Color => {
                self.state.reset_color_scope();
                // The sidebar mirror is applied after the menu; reset it too so
                // the stale mapping cannot repaint what we just cleared.
                self.controls.reset_coloring();
            }
            EditScope::Camera => {
                menu::reset_camera_pose(&mut self.orbit, &mut self.camera, &mut self.fly);
                self.fly_keys.clear();
                self.presets.clear_active();
            }
            EditScope::Simulation => self.reset_simulation(),
            EditScope::All => self.reset_all_document(),
        }
        if !scope.records_history() {
            return;
        }
        let after = capture_document(&self.state, &self.orbit);
        if after != before {
            let label = match scope {
                EditScope::Color => "reset colour",
                EditScope::Camera => "reset camera",
                EditScope::All => "reset document",
                EditScope::Simulation => unreachable!("checked above"),
            };
            // A reset is its own action, never a continuation of an open
            // pointer gesture.
            self.history.cancel();
            self.history.record_named(before, &after, label);
        }
        self.last_committed = after;
    }

    /// Runtime-only reset: simulation time to zero, particles cleared, click
    /// streamlines cleared. Not recorded.
    fn reset_simulation(&mut self) {
        self.clock.reset_time();
        self.state.set_time(0.0);
        clear(&mut self.particles);
        self.particle_target = None;
        self.click_seeds.clear();
        self.last_click_seed = None;
        self.click_lines.clear();
        self.streamline_count = self.flow_lines.len();
        if let Some(scene) = self.scene.as_vector_mut() {
            scene.lines = Arc::new(compose_lines_with_frame_style(
                &self.flow_lines,
                &self.click_lines,
                &self.state.scene_domain(),
                self.state.is_3d(),
                self.state.plane_frame(),
                self.state.show_grid,
                self.state.show_coordinate_system,
                &self.state.reference_viewport_style(),
            ));
        }
    }

    /// Whole-document reset: state plus the app-owned camera and cached scalar
    /// view. Layout, profiles, the clock and particles stay untouched.
    fn reset_all_document(&mut self) {
        self.state.reset_document();
        self.controls = Controls::new();
        self.orbit = OrbitCamera::default();
        self.camera = Camera::default();
        self.fly = None;
        self.fly_keys.clear();
        self.presets.clear_active();
        self.reset_plot_state();
        self.bar.text = self.state.active_source().to_string();
    }
}

#[doc(hidden)]
impl VfApp {
    /// Test seam: the current undo depth.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Test seam: the current redo depth.
    pub fn history_redo_len(&self) -> usize {
        self.history.redo_len()
    }

    /// Test seam: the label the next undo would name.
    pub fn next_undo_label(&self) -> Option<&str> {
        self.history.undo_label()
    }

    /// Test seam: the label the next redo would name.
    pub fn next_redo_label(&self) -> Option<&str> {
        self.history.redo_label()
    }

    /// Test seam: the document snapshot the app would record now.
    pub fn capture_document_for_test(&self) -> EditSnapshot {
        capture_document(&self.state, &self.orbit)
    }

    /// Test seam: apply a snapshot through the production seam.
    pub fn apply_document_for_test(&mut self, snapshot: &EditSnapshot) {
        self.apply_snapshot(snapshot);
    }

    /// Test seam: dispatch one menu action the way the top bar does.
    pub fn apply_menu_action_for_test(&mut self, action: MenuAction, ctx: &egui::Context) {
        self.apply_menu_action(action, ctx);
    }
}
