use std::sync::{Arc, Mutex};

use eframe::egui;
use glam::{Mat4, Vec2, Vec3};

use crate::animation::AnimationError;
use crate::animation_job::{AnimationEligibility, AnimationJob, AnimationJobState};
use crate::animation_scene::AnimationSnapshot;
use crate::camera::{OrbitCamera, ViewCamera, apply_camera_record};
use crate::color_mapping::{ClipCounts, ColorMapping, ColorSample, ColorStats};
use crate::domain::Domain;
use crate::edit_history::{EditHistory, EditSnapshot};
use crate::fly_camera::{
    CROSSHAIR_ALPHA, FlyCamera, FlyKeys, capture_exit_required, capture_transition,
    crosshair_segments, crosshair_visible, exit_on_focus_loss, frame_look, is_command_chord,
    sync_fly, sync_fly_mirror,
};
use crate::interaction::{self, InteractionProfile};
use crate::layer_style::{LayerId, LayerStyles};
use crate::object_scene::MeshScene;
use crate::persistence::{self, AppSettings, PresetUiRecord, ViewRecord};
use crate::plane::PlaneFrame;
use crate::plot::PlotKind;
use crate::plot_camera::PlotCamera;
use crate::plot_scene::{PlotScene, ViewportScene};
use crate::profile_library::ProfileLibrary;
use crate::reference::{reference_lines_visible_with, reference_plane_lines_with};
use crate::render::Camera;
use crate::renderer::{
    ArrowInstanceGPU, DrawCallback, DrawState, LineInstanceGPU, SceneGPU, combined_instances,
    sort_scene_back_to_front, sorted_arrows_back_to_front, sorted_lines_back_to_front,
};
use crate::screenshot::{self, Instant, Screenshot};
use crate::seed_layout_library::SeedLayoutLibrary;
use crate::sim_clock::SimClock;
use crate::source_scene::FieldMode;
use crate::streamlines::click_seed;
use crate::tour::{PanelId, TourProgress};
use crate::ui::animation_dialog::{self, AnimationDialogAction, AnimationDialogState};
use crate::ui::appearance_panel::AppearancePanelState;
use crate::ui::formula_bar::FormulaBarState;
use crate::ui::griffiths::GriffithsState;
use crate::ui::induction_panel::InductionPanelState;
use crate::ui::interaction_panel::InteractionPanel;
use crate::ui::layout::{self, LayoutMode, LayoutState, SheetSize, SheetState};
use crate::ui::material_panel::MaterialPanelState;
use crate::ui::maxwell_panel::MaxwellPanelState;
use crate::ui::menu::{self, MenuAction};
use crate::ui::ode_panel::OdePanelState;
use crate::ui::presets::PresetPanel;
#[cfg(not(target_arch = "wasm32"))]
use crate::ui::presets::default_preset_dir;
use crate::ui::sidebar::Controls;
use crate::ui::sources_panel::SourcesPanelState;
use crate::ui::status_bar::{FpsMeter, advance_hint, restart_hint};
use crate::ui::style_panel::{self, AppStyle};
use crate::ui::top_bar;
use crate::ui::tour_overlay::TourFrameRects;
use crate::view_command::{self, ViewCommand, ViewContext};
use crate::viewmodel::{
    CapturedClickOutcome, ClickOutcome, DragButtons, DragChannel, LastClickSeed, LeftClickAction,
    State, ViewGesture, apply_camera_input, captured_click_outcome, click_outcome, drag_channel,
    two_finger_pan, undo_click_seed,
};
use crate::viewport_style::ViewportStyle;

// Particle cloud (ticket 05 / plan task 10): the viewmodel owns the toggle,
// the app owns the cloud and decides when to run these pure functions.
use crate::particles::{
    AdvectSettings, CLICK_BURST_PARTICLES, ParticlePalette, Particles, advect, clear,
    count as particle_count, dot_instances_styled_at, retarget, spawn, spawn_at, speed_range,
    trail_instances_styled_at,
};

mod frame;
mod history;
mod input;
mod panels;
mod plane;
pub mod plot;
mod present;

use plane::SceneFrameKey;
mod simulation;
mod viewport;

/// Directory where the app persists camera presets. Kept in one place so the
/// panel and any export helper always agree (preset-persistence plan Task 3).
#[cfg(not(target_arch = "wasm32"))]
pub fn presets_data_dir() -> std::path::PathBuf {
    default_preset_dir()
}

/// The scene and camera captured before a temporary Griffiths activity opens.
/// Back/Exit restores it; Keep exploring drops it and adopts the activity scene.
struct GriffithsStart {
    state: State,
    orbit: OrbitCamera,
    camera_position: [f32; 2],
    camera_scale: f32,
    history: EditHistory<EditSnapshot>,
    last_committed: EditSnapshot,
}

/// What the status bar reports this frame: the latest formula parse state
/// (empty when the field is good), a transient hint that replaces "ready"
/// while live, and the live frame/scene readout. Counts are snapshot-derived:
/// arrows, particles and trail segments come from the built scene, while
/// streamlines stay the app's field-geometry segment count (the scene's line
/// collection also holds the decorative backdrop).
#[derive(Debug, Clone, Default)]
pub struct StatusBarState {
    pub parse_message: String,
    /// Smoothed frames per second; zero until the first valid frame time.
    pub fps: f32,
    /// Smoothed frame time in milliseconds; zero until the first valid frame.
    pub frame_ms: f32,
    pub arrow_count: usize,
    /// Grid + click streamline segments, excluding the backdrop lines.
    pub streamline_count: usize,
    pub particle_count: usize,
    pub trail_count: usize,
    pub density: u32,
    /// Labels the density as `n³` (3D sampling) or `n²` (2D).
    pub is_3d: bool,
    /// Simulated seconds while the transport is visible; `None` otherwise.
    /// Paused clocks keep showing their frozen time.
    pub sim_time: Option<f32>,
    /// The fly camera is active, so the readout gets a `fly` prefix.
    pub flying: bool,
    /// Transient one-line hint (ticket 60 sets it); the parse error still wins.
    /// Set it only on the triggering event, never every frame: the app starts a
    /// fresh 2.5 s frame-time lifetime when the text changes, so re-setting the
    /// same message each frame would make it flicker instead of expiring once.
    pub hint: Option<String>,
    /// Live animation export progress, e.g. `Animation 12/48 frames`.
    pub animation_status: Option<String>,
}

/// How many viewport clicks keep their seeded bursts. Clicks accumulate only up
/// to this bound (the oldest burst is dropped), so repeated clicking cannot
/// grow the scene without limit.
const MAX_CLICK_SEEDS: usize = 6;
// Double-click seed handling (ticket 18): the first click seeds immediately and
// a second click within `viewmodel::DOUBLE_CLICK_WINDOW` takes that burst back
// before resetting the view. This is an accepted transient — a deferred seed
// would add ~300 ms of latency to every single click, while the final state
// after a double-click is clean. A triple-click is not special-cased: egui
// reports the third click as a plain click, so it seeds again after the reset.
/// Application root state: the viewmodel State (the single source of truth for
/// what is on screen), the formula bar's local widgets, the sidebar's local
/// Controls, the status readouts, both cameras (the 2D `Camera` and the
/// `OrbitCamera`), the preset manager, the persisted appearance and layout
/// records, and the GPU scene currently being painted.
pub struct VfApp {
    pub state: State,
    pub bar: FormulaBarState,
    pub controls: Controls,
    /// Panel-local drafts for the Sources rail/sheet; invalid text survives
    /// frames without touching the committed scene.
    pub sources: SourcesPanelState,
    /// Panel-local draft for the Induction rail/sheet; only valid edits reach
    /// the committed scene.
    pub induction: InductionPanelState,
    /// Panel-local draft for the ODE rail/sheet; only valid edits reach the
    /// committed scene.
    pub ode: OdePanelState,
    /// Panel-local draft for the Materials rail/sheet; only valid edits reach
    /// the committed scene.
    pub material: MaterialPanelState,
    /// Panel-local draft for the Maxwell rail/sheet; only valid edits reach the
    /// committed scene.
    pub maxwell: MaxwellPanelState,
    /// Effective width for the later Presets phase; never persisted.
    wide_presets_width: f32,
    /// Effective width for the later Appearance phase; never persisted.
    wide_appearance_width: f32,
    /// Effective width for the later Interaction phase; never persisted.
    wide_interaction_width: f32,
    /// The right Interaction panel's session state (issue #88 slice A2b).
    /// Binding/feel edits commit through `VfApp::interaction` and
    /// `VfApp::interaction_profiles`; nothing here is persisted.
    pub interaction_panel: InteractionPanel,
    #[cfg(not(target_arch = "wasm32"))]
    pub cas: crate::ui::cas_panel::CasPanelState,
    pub status: StatusBarState,
    /// Smoothed FPS/frame-time source, fed once per frame from `stable_dt`.
    pub fps_meter: FpsMeter,
    /// Bounded native present-path measurement (#18 task b). `None` in every
    /// ordinary run; only `main` opts in through `VFE_PERF_PRESENT=1`.
    present: Option<present::PresentRun>,
    /// Frame time accumulated while the current transient hint is live.
    hint_elapsed: f32,
    /// The hint text observed last frame, so a new hint restarts its lifetime.
    hint_seen: Option<String>,
    /// The 2D camera record the preset panel saves/applies; the live view is
    /// the shared [`OrbitCamera`] (its ortho mode is the 2D view).
    pub camera: Camera,
    pub orbit: OrbitCamera,
    /// The active six-DOF fly camera (ticket 46). `None` means the orbit
    /// camera is showing; entering fly snapshots the orbit pose, exiting drops
    /// this and the untouched orbit view returns. Never persisted.
    pub fly: Option<FlyCamera>,
    /// Movement keys held this frame, accumulated from `Event::Key` so the
    /// physical QWERTY positions drive movement on native.
    pub fly_keys: FlyKeys,
    /// Whether pointer capture is active (ticket 65): the last state sent to
    /// the platform. The commands are edge-triggered from this flag, never
    /// every frame.
    capture_active: bool,
    /// The click that entered fly (the sidebar checkbox, normally) must not
    /// also seed the field through the captured-click path.
    suppress_capture_click: bool,
    pub presets: PresetPanel,
    /// The right Appearance panel's inline error/readout state (issue #84
    /// slice A2b). The committed styles live in [`State::layer_styles`].
    pub appearance: AppearancePanelState,
    /// The persisted user profile library (issue #85 slice A2a); built-ins are
    /// not stored. Restored from eframe storage on startup without applying a
    /// profile, and written back in [`eframe::App::save`]. The Appearance panel
    /// edits it in slice A2b.
    pub profiles: ProfileLibrary,
    /// The active interaction bindings and camera feel (issue #88 slice A2a).
    /// Restored from eframe storage without applying a profile, and written
    /// back in [`eframe::App::save`]. Input routing reads it in slice A3.
    pub interaction: InteractionProfile,
    /// The persisted user interaction profile library (issue #88 slice A2a);
    /// the built-in `Default` is not stored. Restored without applying a
    /// profile and written back in [`eframe::App::save`]. The Interaction panel
    /// edits it in slice A2b.
    pub interaction_profiles: interaction::ProfileLibrary,
    /// The persisted user seed-layout library (issue #87 slice S3a); the
    /// built-in empty `Default` is not stored. Restored from eframe storage
    /// without applying a layout or dirtying the scene, and written back in
    /// [`eframe::App::save`]. The Flow-section UI edits it in slice S3b.
    pub seed_layouts: SeedLayoutLibrary,
    /// The persisted look-and-feel record (ticket 07): restored from eframe's
    /// app-state storage on startup and written back in [`eframe::App::save`].
    /// It lives app-side and never touches the viewmodel `State`.
    pub style: AppStyle,
    /// The persisted layout record (ticket 14): panel visibility, expanded
    /// widths and help-overlay visibility.
    pub layout: LayoutState,
    /// The layout assembly for this frame, recomputed from the root width once
    /// at the top of `App::ui` before any panel exists (ticket 53). Transient:
    /// never written to `LayoutState`, `AppSettings` or `ViewRecord`.
    pub layout_mode: LayoutMode,
    /// The compact bottom sheet open this frame (ticket 53). Transient: the
    /// wide layout ignores it and `Reset layout` closes it without persisting.
    pub sheet: SheetState,
    /// Transient height state of the compact sheet (ticket 71). Never persisted.
    pub sheet_expanded: SheetSize,
    /// Bumped by "Reset layout" so the panels get fresh egui ids and therefore
    /// forget their last dragged size, letting the default widths apply.
    panel_generation: u32,
    /// Whether the Appearance popover is up (ticket 15; one entry point since
    /// ticket 59).
    appearance_open: bool,
    /// Whether the Help > About dialog is up (ticket 15).
    about_open: bool,
    /// Whether the Help > Keyboard & mouse window is up (ticket 18): the full
    /// help table, sharing `HELP_SECTIONS` with the viewport overlay.
    help_open: bool,
    /// Session-only guided-tour progress (#99 slice B). `Some` keeps the tour
    /// active or finished so the Help entry can restart it; nothing here is
    /// persisted or recorded in history.
    pub tour: Option<TourProgress>,
    /// Issue #223 phase 1: the optional Griffiths branch, its four-route picker
    /// and the ring/shell/wire lessons. Session-only; never persisted or
    /// recorded in history.
    pub griffiths: GriffithsState,
    /// The scene and camera captured before a Griffiths lesson opened, used by
    /// Back/Exit. `None` outside the lesson or after Keep exploring adopts.
    griffiths_start: Option<GriffithsStart>,
    /// The value snapshotted for an `AdvanceRule::Changed` stop, keyed by the
    /// stop index so it is recaptured exactly when that stop becomes current.
    tour_snapshot: Option<(usize, Option<String>)>,
    /// Real panel/viewport rectangles recorded during this frame. Transient;
    /// cleared at the start of every frame.
    tour_rects: TourFrameRects,
    /// The card rectangle painted this frame, for compact-layout coverage.
    tour_card_rect: Option<egui::Rect>,
    /// The real target rectangle, or `None` when the anchor did not resolve.
    tour_target_rect: Option<egui::Rect>,
    /// The stop index whose target panel the tour has already opened. A stop
    /// opens its panel once; if the user closes it, it stays closed until the
    /// stop changes.
    tour_prepared_index: Option<usize>,
    /// How many times `Fit` was applied to the orbit camera. The viewport stop
    /// waits on this rather than the camera pose, so orbit, pan and zoom stay
    /// free exploration while pressing Fit continues, as the card says.
    camera_fit_generation: u64,
    /// The viewport rect from this frame (set in the central panel) so the help
    /// overlay can anchor to its bottom-left corner.
    last_viewport_rect: Option<egui::Rect>,
    /// The one cached GUI scene: the vector snapshot or the scalar plot scene.
    /// The CLI keeps its own `SceneGPU` and never reads this.
    scene: ViewportScene,
    /// The cached object scene for the current source/induction mode. The
    /// stored `Arc` is replaced only when its content fingerprint changes, so
    /// the pass can skip its instance upload on an unchanged frame.
    object_scene: Arc<MeshScene>,
    /// The transient plot-only camera, independent of the saved vector orbit.
    plot_camera: PlotCamera,
    /// The vector orbit/2D pose remembered while plotting. Never persisted:
    /// scalar navigation must not move the camera the user returns to.
    saved_vector_orbit: Option<OrbitCamera>,
    /// Last observed `State::plot.transition_epoch`, so one kind change is
    /// reconciled exactly once per frame boundary.
    last_plot_epoch: u64,
    /// Whether the previous transition-relevant frame was vector mode.
    last_plot_was_vector: bool,
    /// `State::plot.generation` the cached plot scene was built from.
    plot_scene_generation: u64,
    /// Callback prepare/paint errors, published by `PlotCallback` because the
    /// egui-wgpu trait cannot return a Result.
    plot_errors: Arc<Mutex<Option<String>>>,
    /// Readable plot status for the UI/status bar: fit failures, callback
    /// errors and unsupported-install diagnostics. Never a vector fallback.
    plot_status: String,
    /// Whether the live `PlotDrawState` is installed in the current renderer.
    plot_renderer_ready: bool,
    /// Leaving plot mode asks the next viewport frame to drop the large plot
    /// color/depth targets while the device is available.
    plot_release_pending: bool,
    renderer_ready: bool,
    streamline_count: usize,
    /// Source-marker glyph segments appended to the latest vector scene's
    /// composed lines. Zero in Formula mode; the app owns the count because
    /// the snapshot's line collection holds only formula streamlines.
    source_glyph_line_count: usize,
    /// The advected cloud behind the "Particles" toggle (ticket 05): 3D-view
    /// only; its per-frame dots ramp with `dots_range`, the same magnitude
    /// range the arrows were coloured with at the last scene rebuild.
    particles: Particles,
    /// The effective particle count last applied to the cloud (ticket 30).
    /// `None` while particles are off/in 2D. The simulation only resizes when
    /// this changes, so a click burst above the configured count is not
    /// trimmed on the very next frame.
    particle_target: Option<usize>,
    /// The one simulation clock (ticket 26): the only time source for the
    /// particle cloud. Transient by design — never persisted, so every session
    /// starts at time 0, playing, 1x.
    pub clock: SimClock,
    dots_range: (f32, f32),
    /// Particle colour inputs cached with the scene rebuild. `dots_range` is
    /// kept separately for click streamlines, which only need the range.
    color_facts: SceneColorFacts,
    /// Where the user clicked in the viewport (ticket 06), bounded by
    /// [`MAX_CLICK_SEEDS`] and oldest-first. The viewmodel turns each pick into
    /// a deterministic streamline burst; the newest click also drops a
    /// particle burst when the particle toggle is on.
    click_seeds: Vec<Vec3>,
    /// The most recent single click's seed, so a double-click can take it back
    /// (ticket 18: double-click resets the view, it must not seed).
    last_click_seed: Option<LastClickSeed>,
    /// The grid-seeded streamline segments from the last scene rebuild, kept
    /// separately so a click can recompose the line collection without
    /// re-sampling or re-tracing the grid.
    flow_lines: Vec<LineInstanceGPU>,
    /// The current click bursts' streamline segments.
    click_lines: Vec<LineInstanceGPU>,
    /// Ode's trajectory + axis lines, rebuilt once per solved scene/edit and
    /// reused every frame. Empty outside Ode.
    ode_lines: Arc<Vec<LineInstanceGPU>>,
    /// The Ode epoch whose lines are cached; `Some` after the first Ode frame.
    last_ode_epoch: Option<u64>,
    /// Whether the first solution after entering Ode may auto-fit the camera.
    /// Cleared after that frame so later epochs preserve the camera.
    ode_autofit_pending: bool,
    /// The screenshot action (ticket 08): the pending capture request, the
    /// channel its save thread reports on, and the completion toast.
    pub screenshot: Screenshot,
    /// The animation export dialog's settings and progress.
    pub animation: AnimationDialogState,
    /// The single running animation job, if any. It owns a detached snapshot;
    /// the live scene is never stepped by it.
    pub animation_job: Option<AnimationJob>,
    /// The transport's playing state before the export paused it. Restored on
    /// Done, Cancelled and error.
    animation_restore_playing: Option<bool>,
    /// One async offscreen readback in flight on the browser: the callback
    /// sends RGBA here and the next app update pushes it to the job. Always
    /// present so the clearing rule has a native test seam; only the wasm
    /// update path ever populates it.
    animation_wasm_rx: Option<std::sync::mpsc::Receiver<Result<Vec<u8>, String>>>,
    /// Provenance claimed with the SVG request at execution dispatch. `None`
    /// means the active request had no source context.
    svg_export_provenance: Option<crate::export_provenance::ExportProvenance>,
    /// The visible frame the cached scene and click seeds belong to (issue 13
    /// task 6); reconciled at the start of every vector simulation frame.
    scene_frame: SceneFrameKey,
    /// Whether the previous frame had particle availability (a three-component
    /// field); a true-to-false transition drops the cloud and burst undo.
    particle_available: bool,
    /// The field mode observed last frame; a change clears formula transients.
    last_field_mode: FieldMode,
    /// Issue #86 slice A2: the bounded, memory-only edit history. Never
    /// persisted.
    history: EditHistory<EditSnapshot>,
    /// The document the last frame observed, so a menu/shortcut/preset/load
    /// change can be recorded directly outside a pointer gesture.
    last_committed: EditSnapshot,
}
impl VfApp {
    /// App with no persistent storage (defaults for every persisted record).
    pub fn new() -> Self {
        Self::new_with_storage(None)
    }

    /// App wired to eframe's persistent storage: the settings envelope
    /// (style + layout + view state) is restored from it — absent, corrupt or
    /// older records fall back to sensible defaults — and written back on save.
    ///
    /// Native callers that own the egui context should use
    /// [`Self::new_with_storage_and_context`] so the KaTeX faces are registered
    /// before the first frame.
    pub fn new_with_storage(storage: Option<&dyn eframe::Storage>) -> Self {
        Self::build_with_storage(storage)
    }

    /// Native construction path: register the typeset faces on the real egui
    /// context before any widget lays text out, then build the app normally.
    /// Registration is idempotent, so this cannot duplicate font data.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_with_storage_and_context(
        storage: Option<&dyn eframe::Storage>,
        ctx: &eframe::egui::Context,
    ) -> Self {
        crate::ui::math_view::install_math_fonts(ctx);
        Self::build_with_storage(storage)
    }

    /// Native present-path measurement entry (#18 task b).
    ///
    /// No app storage is passed, so settings/history are in-memory and never
    /// loaded or written; presets and saved scenes may still be read
    /// read-only by their panels, but a measurement never overwrites them.
    /// The named scene is applied before the view is derived.
    ///
    /// # Panics
    ///
    /// `config.scene` must be one of
    /// [`SUPPORTED_SCENES`](crate::perf_present::SUPPORTED_SCENES), as
    /// [`PresentConfig::from_env`](crate::perf_present::PresentConfig::from_env)
    /// guarantees; a hand-built invalid config panics rather than measuring a
    /// different scene.
    pub fn new_present(config: crate::perf_present::PresentConfig) -> Self {
        Self::build_with_storage_and_present(None, Some(config))
    }

    /// Test seam: whether this instance opted into the present harness.
    #[doc(hidden)]
    pub fn present_mode_active(&self) -> bool {
        self.present.is_some()
    }

    fn build_with_storage(storage: Option<&dyn eframe::Storage>) -> Self {
        Self::build_with_storage_and_present(storage, None)
    }

    fn build_with_storage_and_present(
        storage: Option<&dyn eframe::Storage>,
        present: Option<crate::perf_present::PresentConfig>,
    ) -> Self {
        let settings = persistence::load(storage);
        let mut state = State::new(false);
        let mut controls = Controls::new();
        // Restore through the viewmodel setters so the first frame samples the
        // saved field; a corrupt record cannot poison the grid. The durable
        // parameter set is assigned first by `AppSettings::restore` (and does
        // not dirty the scene on its own), so a saved parameterised formula
        // binds the saved values.
        settings.restore(&mut state, &mut controls);
        // Present measurements apply their named scene through the shipped
        // state constructors/setters after restoration and before the orbit,
        // backdrop and cached collections are derived from it.
        if let Some(config) = &present {
            crate::perf_present::apply_scene(&mut state, &config.scene)
                .expect("PresentConfig validated its scene");
        }
        // `AppSettings::restore` also applies the persisted object style through
        // `State::restore_object_style`, which never dirties the first frame.
        // Same for the viewport style: startup assigns the loaded record
        // directly and does not dirty the scene on its own. `load` already
        // sanitized corrupt values.
        state.viewport_style = settings.viewport_style;
        // The same direct startup assignment for layer styles: restoring them
        // must not dirty the scene. `load` already repaired invalid values.
        state.layer_styles = settings.layer_styles;
        // Ticket 47: a restored non-default domain must already be framed on
        // the first frame, not only after the first edit. This is the same
        // tested recentre helper the live domain editor uses; the default
        // domain's centre is the origin, so a fresh session is unchanged.
        let mut orbit = OrbitCamera {
            up_axis: state.up_axis,
            ..OrbitCamera::default()
        };
        if state.mode_3d {
            orbit.recentre_on(&state.scene_domain());
        } else {
            orbit.recentre_on_2d(&state.scene_domain());
        }
        let backdrop = compose_lines_with_frame_style(
            &[],
            &[],
            &state.scene_domain(),
            state.is_3d(),
            state.plane_frame(),
            state.show_grid,
            state.show_coordinate_system,
            &state.reference_viewport_style(),
        );
        let arrow_style = state.arrow_style;
        let arrow_style_2d = state.arrow_style_2d;
        let arrow_params = state.effective_arrow_params();
        let mode_3d = state.mode_3d;
        let scene_frame = SceneFrameKey::of(&state);
        let particle_available = state.field.dimension == 3;
        let last_plot_epoch = state.plot.transition_epoch;
        let last_plot_was_vector = state.is_vector_mode();
        let last_field_mode = state.field_mode;
        let last_ode_epoch = Some(state.ode_epoch);
        // A restored Materials record must seed the panel draft from the
        // committed scene, exactly like entering the mode; otherwise the panel
        // would show a stale default on the first frame.
        let mut material = MaterialPanelState::default();
        if state.field_mode == FieldMode::Materials {
            material.sync_from(&state);
        }
        let mut maxwell = MaxwellPanelState::default();
        if state.field_mode == FieldMode::Maxwell {
            maxwell.sync_from(&state);
        }
        let saved_vector_orbit = if last_plot_was_vector {
            None
        } else {
            Some(orbit)
        };
        // Seed the edit-history baseline from the restored document, so the
        // first frame does not record startup restoration as a user edit.
        let last_committed = history::capture_document(&state, &orbit);
        // Restore the persisted group expansion before the first frame without
        // reporting a layout edit; the panel's search and filters stay fresh.
        let mut presets = PresetPanel::new();
        presets.restore_expanded(settings.preset_ui.expanded);
        // The persisted user profile library (issue #85 slice A2a). `load` has
        // already sanitized, de-duplicated and capped the stored records, so
        // construction only fails on an internal bug; start empty rather than
        // panic. Restoring the library never applies a profile or dirties the
        // scene.
        let profiles = ProfileLibrary::new(settings.appearance_profiles).unwrap_or_else(|error| {
            log::warn!("persisted appearance profiles could not be loaded: {error}");
            ProfileLibrary::default()
        });
        // The active interaction record and user library (issue #88 slice A2a).
        // `load` has already repaired the active record and sanitized, dropped,
        // de-duplicated and capped the stored profiles, so construction only
        // fails on an internal bug; start empty rather than panic. Neither
        // restore applies a profile nor dirties the scene.
        let interaction = settings.interaction;
        let interaction_profiles = interaction::ProfileLibrary::new(settings.interaction_profiles)
            .unwrap_or_else(|error| {
                log::warn!("persisted interaction profiles could not be loaded: {error}");
                interaction::ProfileLibrary::default()
            });
        // The persisted named seed-layout library (issue #87 slice S3a). `load`
        // has already sanitised, de-duplicated and capped the stored records
        // against the restored view domain, so construction only fails on an
        // internal bug; start empty rather than panic. Restoring the library
        // never applies a layout or dirties the scene.
        let seed_layouts = SeedLayoutLibrary::new(settings.seed_layouts, &state.scene_domain())
            .unwrap_or_else(|error| {
                log::warn!("persisted seed layouts could not be loaded: {error}");
                SeedLayoutLibrary::default()
            });
        // Issue #171: restore the CAS result-display preference directly. This
        // is a display setting and never dirties the scene.
        #[cfg(not(target_arch = "wasm32"))]
        let mut cas = crate::ui::cas_panel::CasPanelState::default();
        #[cfg(not(target_arch = "wasm32"))]
        {
            cas.typeset_output = settings.view.typeset_output;
        }
        Self {
            state,
            bar: FormulaBarState::new(),
            controls,
            sources: SourcesPanelState::default(),
            induction: InductionPanelState::default(),
            ode: OdePanelState::default(),
            material,
            maxwell,
            wide_presets_width: layout::DEFAULT_PRESETS_WIDTH,
            wide_appearance_width: layout::DEFAULT_APPEARANCE_WIDTH,
            wide_interaction_width: layout::DEFAULT_INTERACTION_WIDTH,
            interaction_panel: InteractionPanel::default(),
            #[cfg(not(target_arch = "wasm32"))]
            cas,
            status: StatusBarState::default(),
            fps_meter: FpsMeter::default(),
            present: present.map(present::PresentRun::new),
            hint_elapsed: 0.0,
            hint_seen: None,
            camera: Camera::default(),
            orbit,
            fly: None,
            fly_keys: FlyKeys::default(),
            capture_active: false,
            suppress_capture_click: false,
            presets,
            appearance: AppearancePanelState::default(),
            profiles,
            interaction,
            interaction_profiles,
            seed_layouts,
            style: settings.style,
            layout: settings.layout,
            layout_mode: LayoutMode::Wide,
            sheet: SheetState::None,
            sheet_expanded: SheetSize::Default,
            panel_generation: 0,
            appearance_open: false,
            about_open: false,
            help_open: false,
            tour: None,
            griffiths: GriffithsState::default(),
            griffiths_start: None,
            tour_snapshot: None,
            tour_rects: TourFrameRects::new(),
            tour_card_rect: None,
            tour_target_rect: None,
            tour_prepared_index: None,
            camera_fit_generation: 0,
            last_viewport_rect: None,
            scene: ViewportScene::Vector(SceneGPU {
                arrows: Arc::new(Vec::new()),
                lines: Arc::new(backdrop),
                trails: Arc::new(Vec::new()),
                dots: Arc::new(Vec::new()),
                combined: Arc::new(Vec::new()),
                heatmap: None,
                arrow_style,
                arrow_style_2d,
                arrow_params,
                mode_3d,
            }),
            object_scene: Arc::new(MeshScene::empty()),
            plot_camera: PlotCamera::default(),
            saved_vector_orbit,
            last_plot_epoch,
            last_plot_was_vector,
            plot_scene_generation: 0,
            plot_errors: Arc::new(Mutex::new(None)),
            plot_status: String::new(),
            plot_renderer_ready: false,
            plot_release_pending: false,
            renderer_ready: false,
            streamline_count: 0,
            source_glyph_line_count: 0,
            particles: Particles::default(),
            particle_target: None,
            clock: SimClock::default(),
            dots_range: (0.0, 1.0),
            color_facts: SceneColorFacts::default(),
            click_seeds: Vec::new(),
            last_click_seed: None,
            flow_lines: Vec::new(),
            click_lines: Vec::new(),
            ode_lines: Arc::new(Vec::new()),
            last_ode_epoch,
            ode_autofit_pending: false,
            screenshot: Screenshot::new(),
            animation: AnimationDialogState::default(),
            animation_job: None,
            animation_restore_playing: None,
            animation_wasm_rx: None,
            svg_export_provenance: None,
            scene_frame,
            particle_available,
            last_field_mode,
            history: EditHistory::new(),
            last_committed,
        }
    }
}

impl Default for VfApp {
    fn default() -> Self {
        Self::new()
    }
}

impl VfApp {
    /// Start the guided tour, or return a running/finished tour to its first
    /// stop. Session-only: no settings, document or history record is touched.
    pub fn start_or_restart_tour(&mut self) {
        match self.tour.as_mut() {
            Some(progress) => progress.restart(),
            None => self.tour = Some(TourProgress::new()),
        }
        self.tour_snapshot = None;
        self.tour_rects.clear();
        self.tour_card_rect = None;
        self.tour_target_rect = None;
        self.tour_prepared_index = None;
    }

    /// Frame phase, before any panel draws: clear last frame's recorded rects
    /// and open the current stop's collapsed target through the existing layout
    /// toggle or compact sheet. A target that still cannot be shown this frame
    /// is simply left unresolved.
    ///
    /// The target is opened once per stop (and again when the stop changes), not
    /// every frame: a user who closes the panel keeps it closed while that stop
    /// is current. The panel is recorded through [`TourProgress::mark_opened`].
    pub(super) fn prepare_tour_frame(&mut self) {
        self.tour_rects.clear();
        self.tour_card_rect = None;
        self.tour_target_rect = None;
        let Some(progress) = self.tour.as_ref() else {
            self.tour_prepared_index = None;
            return;
        };
        let Some(stop) = progress.current() else {
            self.tour_prepared_index = None;
            return;
        };
        let index = progress.index();
        if self.tour_prepared_index != Some(index) {
            if let Some(panel) = crate::ui::tour_overlay::control_panel(stop.target) {
                self.open_tour_panel(panel);
                if let Some(progress) = self.tour.as_mut() {
                    progress.mark_opened(panel);
                }
            }
            self.tour_prepared_index = Some(index);
        }
        self.sync_tour_snapshot();
    }

    /// Open one tour target through the shipped layout path only. In compact
    /// layout that is the existing bottom sheet; in wide layout it is the
    /// panel's own `LayoutState` toggle.
    fn open_tour_panel(&mut self, panel: PanelId) {
        if self.layout_mode == LayoutMode::Compact {
            let sheet = match panel {
                PanelId::Controls => Some(SheetState::Controls),
                PanelId::Presets => Some(SheetState::Presets),
                PanelId::Appearance => Some(SheetState::Appearance),
                PanelId::Interaction => Some(SheetState::Interaction),
                PanelId::Sources => Some(SheetState::Sources),
                PanelId::Induction => Some(SheetState::Induction),
                PanelId::Ode => Some(SheetState::Ode),
                PanelId::Materials => Some(SheetState::Materials),
                PanelId::Maxwell => Some(SheetState::Maxwell),
                PanelId::FormulaBar | PanelId::TopBar | PanelId::StatusBar => None,
            };
            if let Some(sheet) = sheet {
                self.sheet.open(sheet);
                self.sheet_expanded = SheetSize::Default;
            }
            return;
        }

        let expanded = match panel {
            PanelId::Controls => self.layout.controls_expanded,
            PanelId::Presets => self.layout.presets_expanded,
            PanelId::Appearance => self.layout.appearance_expanded,
            PanelId::Interaction => self.layout.interaction_expanded,
            PanelId::Sources => self.layout.sources_expanded,
            PanelId::Induction => self.layout.induction_expanded,
            PanelId::Ode => self.layout.ode_expanded,
            PanelId::Materials => self.layout.materials_expanded,
            PanelId::Maxwell => self.layout.maxwell_expanded,
            PanelId::FormulaBar | PanelId::TopBar | PanelId::StatusBar => return,
        };
        if expanded {
            return;
        }
        match panel {
            PanelId::Controls => self.layout.toggle_controls(),
            PanelId::Presets => self.layout.toggle_presets(),
            PanelId::Appearance => self.layout.toggle_appearance(),
            PanelId::Interaction => self.layout.toggle_interaction(),
            PanelId::Sources => self.layout.toggle_sources(),
            PanelId::Induction => self.layout.toggle_induction(),
            PanelId::Ode => self.layout.toggle_ode(),
            PanelId::Materials => self.layout.toggle_materials(),
            PanelId::Maxwell => self.layout.toggle_maxwell(),
            PanelId::FormulaBar | PanelId::TopBar | PanelId::StatusBar => {}
        }
    }

    /// Record a panel's real painted rectangle for this frame's tour anchor.
    pub(super) fn record_tour_panel(&mut self, panel: PanelId, rect: egui::Rect) {
        self.tour_rects.record_panel(panel, rect);
    }

    /// Record the central viewport's real rectangle.
    pub(super) fn record_tour_viewport(&mut self, rect: egui::Rect) {
        self.tour_rects.record_viewport(rect);
    }

    /// The observable value behind a `Changed` key. Only shipped controls that
    /// a stop waits on need a value; an unknown key stays unresolved.
    fn tour_control_value(&self, key: &str) -> Option<String> {
        Some(match key {
            // Only an applied Fit counts, not the camera pose: orbit, pan and
            // zoom are free exploration, and pressing Fit again on an already
            // framed view still continues. The card says to press Fit domain.
            "camera_fit" => self.camera_fit_generation.to_string(),
            "play" => self.clock.playing.to_string(),
            "maxwell_case" => format!("{:?}", self.state.maxwell.case),
            "formula_input" => self.state.formula.clone(),
            "parameters" => format!("{:?}", self.state.parameters),
            "colormap" => format!("{:?}", self.state.colormap),
            "streamlines" => self.state.streamlines.to_string(),
            "particles" => self.state.particles.to_string(),
            "color_quantity" => format!("{:?}", self.state.color_quantity),
            "view_2d" | "view_3d" => self.state.mode_3d.to_string(),
            _ => return None,
        })
    }

    /// Capture the `Changed` snapshot when the current stop becomes current.
    fn sync_tour_snapshot(&mut self) {
        let Some(progress) = self.tour.as_ref() else {
            self.tour_snapshot = None;
            return;
        };
        let Some(stop) = progress.current() else {
            self.tour_snapshot = None;
            return;
        };
        let index = progress.index();
        let stale = self
            .tour_snapshot
            .as_ref()
            .is_none_or(|(snapshot_index, _)| *snapshot_index != index);
        if stale {
            let value = match stop.advance {
                crate::tour::AdvanceRule::Changed(key) => self.tour_control_value(key),
                crate::tour::AdvanceRule::Next => None,
            };
            self.tour_snapshot = Some((index, value));
        }
    }

    /// Whether the current stop's Next button/key may advance.
    fn tour_advance_ready(&self) -> bool {
        let Some(progress) = self.tour.as_ref() else {
            return false;
        };
        let Some(stop) = progress.current() else {
            return false;
        };
        match stop.advance {
            crate::tour::AdvanceRule::Next => true,
            crate::tour::AdvanceRule::Changed(key) => {
                let captured = self
                    .tour_snapshot
                    .as_ref()
                    .filter(|(index, _)| *index == progress.index())
                    .and_then(|(_, value)| value.as_deref());
                let current = self.tour_control_value(key);
                crate::ui::tour_overlay::advance_ready(stop.advance, captured, current.as_deref())
            }
        }
    }

    fn tour_next(&mut self) {
        if !self.tour_advance_ready() {
            return;
        }
        if let Some(progress) = self.tour.as_mut() {
            progress.advance();
        }
        self.tour_snapshot = None;
    }

    fn tour_back(&mut self) {
        if let Some(progress) = self.tour.as_mut() {
            progress.back();
        }
        self.tour_snapshot = None;
    }

    fn tour_dismiss(&mut self) {
        if let Some(progress) = self.tour.as_mut() {
            progress.dismiss();
        }
        self.tour_snapshot = None;
    }

    /// Consume the tour's keys before the menu, viewport or camera handlers.
    /// A focused text field keeps every key, including Escape, so editing is
    /// never interrupted; the card's own buttons remain available.
    pub(super) fn handle_tour_keys(&mut self, ctx: &egui::Context) {
        if self
            .tour
            .as_ref()
            .is_none_or(|progress| progress.current().is_none())
            || ctx.text_edit_focused()
        {
            return;
        }
        let mut action = None;
        ctx.input_mut(|input| {
            if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                action = Some(crate::ui::tour_overlay::TourAction::Dismiss);
            } else if input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                | input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
            {
                action = Some(crate::ui::tour_overlay::TourAction::Next);
            } else if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                action = Some(crate::ui::tour_overlay::TourAction::Back);
            }
        });
        match action {
            Some(crate::ui::tour_overlay::TourAction::Next) => self.tour_next(),
            Some(crate::ui::tour_overlay::TourAction::Back) => self.tour_back(),
            Some(crate::ui::tour_overlay::TourAction::Dismiss) => self.tour_dismiss(),
            Some(crate::ui::tour_overlay::TourAction::None)
            | Some(crate::ui::tour_overlay::TourAction::Griffiths)
            | None => {}
        }
    }

    /// Frame phase, last: draw the anchored card and highlight, then apply its
    /// one-shot button action.
    pub(super) fn frame_tour_overlay(&mut self, ctx: &egui::Context) {
        self.sync_tour_snapshot();
        if let Some(rect) = self.last_viewport_rect {
            self.record_tour_viewport(rect);
            // Issue #228: the click tool strip sits at the viewport's top edge.
            // It derives from live state and edits only the session controls;
            // the next frame's sidebar handoff applies any prerequisite change.
            let view = crate::ui::viewport_tool::derive_for(&self.state, &self.controls);
            crate::ui::viewport_tool::show(
                ctx,
                rect,
                &view,
                &mut self.controls,
                &self.interaction.bindings,
            );
        }
        let Some(progress) = self.tour.as_ref() else {
            self.tour_card_rect = None;
            self.tour_target_rect = None;
            return;
        };
        if progress.current().is_none() {
            self.tour_card_rect = None;
            self.tour_target_rect = None;
            return;
        }
        let ready = self.tour_advance_ready();
        let outcome = crate::ui::tour_overlay::show(ctx, &self.tour_rects, progress, ready);
        self.tour_card_rect = outcome.card_rect;
        self.tour_target_rect = outcome.target_rect;
        match outcome.action {
            crate::ui::tour_overlay::TourAction::Next => self.tour_next(),
            crate::ui::tour_overlay::TourAction::Back => self.tour_back(),
            crate::ui::tour_overlay::TourAction::Dismiss => self.tour_dismiss(),
            crate::ui::tour_overlay::TourAction::Griffiths => {
                // The optional branch replaces the quick-tour card; the quick
                // tour stays restartable from Help.
                self.tour_dismiss();
                self.open_griffiths_branch();
            }
            crate::ui::tour_overlay::TourAction::None => {}
        }
    }

    /// Test seam: the card rectangle painted this frame, if any.
    #[doc(hidden)]
    pub fn tour_card_rect_for_test(&self) -> Option<egui::Rect> {
        self.tour_card_rect
    }

    /// Test seam: the resolved highlight rectangle, or `None` when the anchor
    /// did not resolve this frame.
    #[doc(hidden)]
    pub fn tour_highlight_rect_for_test(&self) -> Option<egui::Rect> {
        self.tour_target_rect
    }
}

#[doc(hidden)]
impl VfApp {
    /// Test-only: run the Presets finish phase against the live app, the same
    /// seam the frame uses after the panel has drawn.
    pub fn finish_presets_for_test(&mut self) {
        self.finish_presets(None);
    }
}

impl AnimationSnapshot {
    /// Replace the mapped object scene computed by [`Self::from_parts`] with
    /// the live app's cached scene, so a capture shares the exact `Arc`.
    fn with_object_scene(mut self, object_scene: Arc<MeshScene>) -> Self {
        self.object_scene = object_scene;
        self
    }

    /// Copy the live app's animatable state without mutating it.
    pub fn capture(app: &VfApp) -> Result<Self, AnimationError> {
        let clear = {
            let visuals = if app.style.theme_dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            egui::Rgba::from(visuals.panel_fill)
                .to_array()
                .map(f64::from)
        };
        Ok(Self::from_parts(
            app.state.clone(),
            app.particles.clone(),
            app.orbit,
            app.fly,
            clear,
        )
        .with_object_scene(Arc::clone(&app.object_scene)))
    }
}

impl VfApp {
    /// Snapshot the current view for a later animation export.
    pub fn animation_snapshot(&self) -> Result<AnimationSnapshot, AnimationError> {
        AnimationSnapshot::capture(self)
    }

    /// Drop a stale browser readback. A terminal job must never let its old
    /// callback complete against a newer job, and a new job starts with no
    /// receiver in flight.
    fn clear_animation_readback(&mut self) {
        self.animation_wasm_rx = None;
    }
}

/// Fold the domain-matched backdrop, the grid-seeded streamlines and the click
/// bursts into the one line collection the viewport paints. Rebuilt only on a
/// scene rebuild or a click, so the per-frame path still clones an `Arc`. The
/// backdrop follows the current domain and sampling dimension (ticket 23):
/// reference geometry is cheap, and a rebuild is exactly when the domain may
/// have moved.
#[doc(hidden)]
pub fn compose_lines(
    flow: &[LineInstanceGPU],
    clicks: &[LineInstanceGPU],
    domain: &Domain,
    sampling_3d: bool,
    show_grid: bool,
    show_coordinate_system: bool,
) -> Vec<LineInstanceGPU> {
    compose_lines_with_frame(
        flow,
        clicks,
        domain,
        sampling_3d,
        None,
        show_grid,
        show_coordinate_system,
    )
}

/// As [`compose_lines`], but a plane slice supplies its local [`PlaneFrame`]:
/// the backdrop is then the flat local grid and its two world-axis-coloured
/// axes instead of the world cube or mid-z plane. `domain` is ignored for a
/// plane frame, which carries its own origin and extents.
#[doc(hidden)]
pub fn compose_lines_with_frame(
    flow: &[LineInstanceGPU],
    clicks: &[LineInstanceGPU],
    domain: &Domain,
    sampling_3d: bool,
    frame: Option<PlaneFrame>,
    show_grid: bool,
    show_coordinate_system: bool,
) -> Vec<LineInstanceGPU> {
    compose_lines_with_frame_style(
        flow,
        clicks,
        domain,
        sampling_3d,
        frame,
        show_grid,
        show_coordinate_system,
        &ViewportStyle::default(),
    )
}

/// [`compose_lines_with_frame`] with an explicit reference [`ViewportStyle`],
/// so the app can fold in the Reference layer's width and opacity (issue #84
/// A2a). Domain, frame and visibility signals behave exactly as before.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn compose_lines_with_frame_style(
    flow: &[LineInstanceGPU],
    clicks: &[LineInstanceGPU],
    domain: &Domain,
    sampling_3d: bool,
    frame: Option<PlaneFrame>,
    show_grid: bool,
    show_coordinate_system: bool,
    style: &ViewportStyle,
) -> Vec<LineInstanceGPU> {
    let mut lines = match frame {
        Some(frame) => reference_plane_lines_with(&frame, show_grid, show_coordinate_system, style),
        None => reference_lines_visible_with(
            domain,
            sampling_3d,
            show_grid,
            show_coordinate_system,
            style,
        ),
    };
    lines.extend_from_slice(flow);
    lines.extend_from_slice(clicks);
    lines
}

/// Compose the vector scene's line collection exactly as the live viewport
/// does: reference grid/axes, the formula or click streamlines, then the
/// mode-owned source/induction glyphs. Returns the glyph count so the app can
/// keep its live status readout.
#[doc(hidden)]
pub fn compose_vector_lines(
    state: &State,
    flow: &[LineInstanceGPU],
    clicks: &[LineInstanceGPU],
) -> (Vec<LineInstanceGPU>, usize) {
    compose_vector_lines_with(
        state,
        flow,
        clicks,
        state.layer_styles(),
        GlyphExpansion::ObjectBacked,
    )
}

/// Compose the unstyled SVG/vector blueprint: neutral layer records and the
/// full legacy glyph geometry, even when the live object pass is drawing bodies.
/// The app's SVG export never composites objects, so it must keep the complete
/// line-glyph representation regardless of [`crate::object_scene::ObjectStyle`].
#[doc(hidden)]
pub fn compose_vector_lines_blueprint(
    state: &State,
    flow: &[LineInstanceGPU],
    clicks: &[LineInstanceGPU],
) -> (Vec<LineInstanceGPU>, usize) {
    compose_vector_lines_with(
        state,
        flow,
        clicks,
        &LayerStyles::default(),
        GlyphExpansion::Full,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GlyphExpansion {
    /// Suppress body glyphs when the object pass owns them; keep sign markers.
    ObjectBacked,
    /// Always expand the complete legacy glyph (the SVG blueprint contract).
    Full,
}

fn compose_vector_lines_with(
    state: &State,
    flow: &[LineInstanceGPU],
    clicks: &[LineInstanceGPU],
    layers: &LayerStyles,
    expansion: GlyphExpansion,
) -> (Vec<LineInstanceGPU>, usize) {
    let mut lines = compose_lines_with_frame_style(
        flow,
        clicks,
        &state.scene_domain(),
        state.is_3d(),
        state.plane_frame(),
        state.show_grid,
        state.show_coordinate_system,
        &state.reference_viewport_style_for(layers),
    );
    // Issue #114 slice 5: when the object pass draws bodies, the line glyph's
    // body is double geometry. Keep only the polarity marker for the glyphs
    // that the object scene actually mapped; the empty-object line-glyph
    // fallback and the full blueprint expansion keep the legacy glyph.
    let draws_object_bodies = matches!(
        state.object_style(),
        crate::object_scene::ObjectStyle::Solid | crate::object_scene::ObjectStyle::Wireframe
    );
    let object_backed = expansion == GlyphExpansion::ObjectBacked
        && draws_object_bodies
        && !object_scene_for_state(state).objects().is_empty();
    // The status readout keeps reporting the legacy full-glyph count while the
    // vector scene carries only the markers, so the object-backed switch does
    // not change the app's published glyph count.
    let mut legacy_glyph_count = None;
    let glyphs = match state.field_mode {
        FieldMode::Sources => {
            let glyphs = crate::source_glyph::source_glyphs(
                &state.sources,
                &state.scene_domain(),
                state.plane_frame().as_ref(),
            );
            if object_backed {
                let markers = crate::source_glyph::source_sign_marker_lines_with_layer(
                    &glyphs,
                    &state.scene_domain(),
                    layers,
                    LayerId::Sources,
                );
                // The legacy count is the body plus marker length; the split
                // expansion is proven equal in `source_glyph_test`. Avoid a
                // second full legacy expansion whose only use is `.len()`.
                let body_len = crate::source_glyph::source_body_lines_with_layer(
                    &glyphs,
                    &state.scene_domain(),
                    layers,
                    LayerId::Sources,
                )
                .len();
                legacy_glyph_count = Some(body_len + markers.len());
                markers
            } else {
                crate::source_glyph::source_glyph_lines_with(&glyphs, &state.scene_domain(), layers)
            }
        }
        FieldMode::Induction => {
            let mut scene = crate::source_scene::SourceScene::default();
            scene.entries.push(crate::source_scene::SourceEntry {
                enabled: true,
                source: state.induction.unit_source(),
            });
            let glyphs = crate::source_glyph::source_glyphs(
                &scene,
                &state.scene_domain(),
                state.plane_frame().as_ref(),
            );
            if object_backed {
                let markers = crate::source_glyph::source_sign_marker_lines_with_layer(
                    &glyphs,
                    &state.scene_domain(),
                    layers,
                    LayerId::Induction,
                );
                // Same body + marker count as the legacy full expansion, with
                // only the marker geometry retained for the line pass.
                let body_len = crate::source_glyph::source_body_lines_with_layer(
                    &glyphs,
                    &state.scene_domain(),
                    layers,
                    LayerId::Induction,
                )
                .len();
                legacy_glyph_count = Some(body_len + markers.len());
                markers
            } else {
                crate::source_glyph::source_glyph_lines_with_layer(
                    &glyphs,
                    &state.scene_domain(),
                    layers,
                    LayerId::Induction,
                )
            }
        }
        FieldMode::Formula | FieldMode::Materials | FieldMode::Ode | FieldMode::Maxwell => {
            Vec::new()
        }
    };
    let count = legacy_glyph_count.unwrap_or(glyphs.len());
    lines.extend(glyphs);
    // Issue #95 slice B: probe wireframes ride the normal line collection, so
    // PNG, animation and SVG captures include them without a new pass. They
    // follow the mode glyphs so the styled-viewport tail recomposition keeps
    // them without widening the reported glyph count.
    lines.extend(state.probe_overlay_lines(layers));
    (lines, count)
}

/// Map the current source or induction mode to the object scene the renderer
/// composites first. Formula, Materials, Ode and Maxwell have no mapped bodies
/// and return [`MeshScene::empty`], so their render path stays byte-identical.
///
/// The induction branch mirrors [`compose_vector_lines`]: the kind's unit
/// source is mapped through the same [`MeshScene::from_sources`] entry point as
/// the Sources mode, so panel text and the body cannot disagree. Exposed as a
/// test seam: the live viewport, the animation snapshot and the CLI animation
/// path all map through this one function.
#[doc(hidden)]
pub fn object_scene_for_state(state: &State) -> MeshScene {
    let domain = state.scene_domain();
    let frame = state.plane_frame();
    match state.field_mode {
        FieldMode::Sources => MeshScene::from_sources(&state.sources, &domain, frame.as_ref()),
        FieldMode::Induction => {
            let mut scene = crate::source_scene::SourceScene::default();
            scene.entries.push(crate::source_scene::SourceEntry {
                enabled: true,
                source: state.induction.unit_source(),
            });
            MeshScene::from_sources(&scene, &domain, frame.as_ref())
        }
        FieldMode::Formula | FieldMode::Materials | FieldMode::Ode | FieldMode::Maxwell => {
            MeshScene::empty()
        }
    }
}

/// Everything the particle colours need, cached alongside the arrows so dots
/// and trails ramp exactly like the scene's arrows: the visible magnitude
/// range, the mapping's auto-fit stats and the active mapping (if any).
pub(crate) struct SceneColorFacts {
    pub range: (f32, f32),
    pub stats: ColorStats,
    pub clips: ClipCounts,
    pub mapping: Option<ColorMapping>,
}

impl Default for SceneColorFacts {
    fn default() -> Self {
        Self {
            range: (0.0, 1.0),
            stats: ColorStats::default(),
            clips: ClipCounts::default(),
            mapping: None,
        }
    }
}

fn color_samples_from_arrows(arrows: &[crate::sampling::ArrowData]) -> Vec<ColorSample> {
    arrows
        .iter()
        .map(|arrow| ColorSample {
            magnitude: arrow.mag,
            dir: arrow.dir,
        })
        .collect()
}

fn family_color_samples(
    arrows: Option<&[crate::sampling::ArrowData]>,
) -> ((f32, f32), Vec<ColorSample>) {
    match arrows {
        Some(arrows) => (
            crate::sampling::magnitude_range(arrows),
            color_samples_from_arrows(arrows),
        ),
        None => ((0.0, 1.0), Vec::new()),
    }
}

/// The colour range shared by the field scene and its particle dots, matching
/// the live rebuild: Induction/Materials use their own sampling, everything
/// else samples the active field. Stats come from the same drawn arrows as the
/// range, so an auto-fitted mapping cannot disagree with the arrow colours.
pub(crate) fn scene_color_facts(state: &State) -> SceneColorFacts {
    let (range, samples) = match state.field_mode {
        FieldMode::Induction => family_color_samples(
            state
                .induction_sampling
                .as_ref()
                .map(|s| s.arrows.as_slice()),
        ),
        FieldMode::Materials => family_color_samples(
            state
                .material_sampling
                .as_ref()
                .map(|s| s.arrows.as_slice()),
        ),
        FieldMode::Maxwell => {
            family_color_samples(state.maxwell_sampling.as_ref().map(|s| s.arrows.as_slice()))
        }
        _ => {
            let sampled = state.sample();
            (
                state.color_range(&sampled),
                color_samples_from_arrows(&sampled),
            )
        }
    };
    let stats = ColorStats::from_samples(&samples);
    let mapping = state.view_options().color_mapping;
    let clips = mapping
        .as_ref()
        .map_or_else(ClipCounts::default, |mapping| {
            mapping.clip_counts(&samples, &stats)
        });
    SceneColorFacts {
        range,
        stats,
        clips,
        mapping,
    }
}

/// Test seam: integration tests live outside the crate, so expose the facts
/// `scene_color_facts` computes without widening the internal API.
#[doc(hidden)]
pub fn scene_color_facts_for_tests(
    state: &State,
) -> ((f32, f32), ColorStats, Option<ColorMapping>) {
    let facts = scene_color_facts(state);
    (facts.range, facts.stats, facts.mapping)
}

/// Build the live dot and trail instances for a cloud. Shared by the live
/// app and the detached animation snapshot so both paint identical particles.
pub(crate) fn particle_scene_instances(
    state: &State,
    particles: &Particles,
    facts: &SceneColorFacts,
) -> (Arc<Vec<ArrowInstanceGPU>>, Arc<Vec<LineInstanceGPU>>) {
    let appearance = state.particle_appearance.sanitized();
    let spacing = state.spacing();
    let flow = state.scene_flow();
    let domain = state.scene_domain();
    let palette = ParticlePalette {
        colormap: state.colormap,
        color_range: facts.range,
        speed_range: speed_range(particles),
        mapping: facts.mapping.as_ref(),
        stats: &facts.stats,
    };
    let dots = Arc::new(dot_instances_styled_at(
        particles,
        &flow,
        &domain,
        &palette,
        &appearance,
        state.particle_radius(spacing),
        state.time,
        state.flow_layer_style(),
    ));
    let trails = Arc::new(trail_instances_styled_at(
        particles,
        &flow,
        &palette,
        &appearance,
        spacing,
        state.time,
        state.flow_layer_style(),
    ));
    (dots, trails)
}

/// Assemble one frame's renderer scene from the cached vector collections.
/// Ticket 62: the combined stream is the single sorted source for 3D. The
/// arrows stay frame-invariant (no write-back), so equal-depth ties are
/// deterministic. 2D keeps its arrows-then-dots path and paints a stable
/// far-to-near copy of the arrows.
pub(crate) fn assemble_vector_scene(
    state: &State,
    cached: &mut SceneGPU,
    ode_markers: &[ArrowInstanceGPU],
    view: &Mat4,
) -> SceneGPU {
    let combined = if state.mode_3d {
        let mut instances = combined_instances(cached.arrows.as_slice(), cached.dots.as_slice());
        instances.extend(ode_markers.iter().copied());
        sort_scene_back_to_front(&mut instances, view);
        Arc::new(instances)
    } else {
        Arc::new(Vec::new())
    };
    let arrows = if state.mode_3d {
        Arc::clone(&cached.arrows)
    } else {
        Arc::new(sorted_arrows_back_to_front(cached.arrows.as_slice(), view))
    };
    let trails = Arc::new(sorted_lines_back_to_front(cached.trails.as_slice(), view));
    let lines = Arc::clone(&cached.lines);
    let dots = if !state.mode_3d && !ode_markers.is_empty() {
        let mut dots = cached.dots.as_ref().clone();
        dots.extend_from_slice(ode_markers);
        Arc::new(dots)
    } else {
        Arc::clone(&cached.dots)
    };
    let arrow_style = cached.arrow_style;
    let arrow_style_2d = cached.arrow_style_2d;
    let arrow_params = cached.arrow_params;
    let heatmap = cached.heatmap.clone();
    if state.field_mode != FieldMode::Ode {
        cached.combined = Arc::clone(&combined);
    }
    SceneGPU {
        arrows,
        lines,
        trails,
        dots,
        combined,
        heatmap,
        arrow_style,
        arrow_style_2d,
        arrow_params,
        mode_3d: state.mode_3d,
    }
}
