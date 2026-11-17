use std::sync::{Arc, Mutex};

use eframe::egui;
use glam::{Vec2, Vec3};

use crate::camera::{OrbitCamera, ViewCamera, apply_camera_record};
use crate::domain::Domain;
use crate::fly_camera::{
    CROSSHAIR_ALPHA, FlyCamera, FlyKeys, capture_exit_required, capture_transition,
    crosshair_segments, crosshair_visible, exit_on_focus_loss, frame_look, is_command_chord,
    sync_fly, sync_fly_mirror,
};
use crate::persistence::{self, AppSettings, ViewRecord};
use crate::plane::PlaneFrame;
use crate::plot::PlotKind;
use crate::plot_camera::PlotCamera;
use crate::plot_scene::{PlotScene, ViewportScene};
use crate::reference::{reference_lines_visible, reference_plane_lines};
use crate::render::Camera;
use crate::renderer::{
    ArrowInstanceGPU, DrawCallback, DrawState, LineInstanceGPU, SceneGPU, combined_instances,
    sort_scene_back_to_front, sorted_arrows_back_to_front, sorted_lines_back_to_front,
};
use crate::screenshot::{self, Instant, Screenshot};
use crate::sim_clock::SimClock;
use crate::source_scene::FieldMode;
use crate::streamlines::click_seed;
use crate::ui::formula_bar::FormulaBarState;
use crate::ui::induction_panel::InductionPanelState;
use crate::ui::layout::{self, LayoutMode, LayoutState, SheetSize, SheetState};
use crate::ui::material_panel::MaterialPanelState;
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
use crate::view_command::{self, ViewCommand, ViewContext};
use crate::viewmodel::{
    CapturedClickOutcome, ClickOutcome, DragButtons, DragChannel, LastClickSeed, LeftClickAction,
    State, ViewGesture, apply_camera_input, captured_click_outcome, click_outcome, drag_channel,
    two_finger_pan, undo_click_seed,
};

// Particle cloud (ticket 05 / plan task 10): the viewmodel owns the toggle,
// the app owns the cloud and decides when to run these pure functions.
use crate::particles::{
    AdvectSettings, CLICK_BURST_PARTICLES, Particles, advect, clear, count as particle_count,
    dot_instances_at, retarget, spawn, spawn_at, speed_range, trail_instances_at,
};

mod frame;
mod input;
mod panels;
mod plane;
pub mod plot;

use plane::SceneFrameKey;
mod simulation;
mod viewport;

/// Directory where the app persists camera presets. Kept in one place so the
/// panel and any export helper always agree (preset-persistence plan Task 3).
#[cfg(not(target_arch = "wasm32"))]
pub fn presets_data_dir() -> std::path::PathBuf {
    default_preset_dir()
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
    /// Effective width for the later Presets phase; never persisted.
    wide_presets_width: f32,
    #[cfg(not(target_arch = "wasm32"))]
    pub cas: crate::ui::cas_panel::CasPanelState,
    pub status: StatusBarState,
    /// Smoothed FPS/frame-time source, fed once per frame from `stable_dt`.
    pub fps_meter: FpsMeter,
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
    /// The viewport rect from this frame (set in the central panel) so the help
    /// overlay can anchor to its bottom-left corner.
    last_viewport_rect: Option<egui::Rect>,
    /// The one cached GUI scene: the vector snapshot or the scalar plot scene.
    /// The CLI keeps its own `SceneGPU` and never reads this.
    scene: ViewportScene,
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
}
impl VfApp {
    /// App with no persistent storage (defaults for every persisted record).
    pub fn new() -> Self {
        Self::new_with_storage(None)
    }

    /// App wired to eframe's persistent storage: the settings envelope
    /// (style + layout + view state) is restored from it — absent, corrupt or
    /// older records fall back to sensible defaults — and written back on save.
    pub fn new_with_storage(storage: Option<&dyn eframe::Storage>) -> Self {
        let settings = persistence::load(storage);
        let mut state = State::new(false);
        let mut controls = Controls::new();
        // Restore through the viewmodel setters so the first frame samples the
        // saved field; a corrupt record cannot poison the grid.
        settings.view.restore(&mut state, &mut controls);
        // Ticket 47: a restored non-default domain must already be framed on
        // the first frame, not only after the first edit. This is the same
        // tested recentre helper the live domain editor uses; the default
        // domain's centre is the origin, so a fresh session is unchanged.
        let mut orbit = OrbitCamera::default();
        if state.mode_3d {
            orbit.recentre_on(&state.scene_domain());
        } else {
            orbit.recentre_on_2d(&state.scene_domain());
        }
        let backdrop = compose_lines_with_frame(
            &[],
            &[],
            &state.scene_domain(),
            state.is_3d(),
            state.plane_frame(),
            state.show_grid,
            state.show_coordinate_system,
        );
        let arrow_style = state.arrow_style;
        let arrow_style_2d = state.arrow_style_2d;
        let arrow_params = state.arrow_params;
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
        let saved_vector_orbit = if last_plot_was_vector {
            None
        } else {
            Some(orbit)
        };
        Self {
            state,
            bar: FormulaBarState::new(),
            controls,
            sources: SourcesPanelState::default(),
            induction: InductionPanelState::default(),
            ode: OdePanelState::default(),
            material,
            wide_presets_width: layout::DEFAULT_PRESETS_WIDTH,
            #[cfg(not(target_arch = "wasm32"))]
            cas: crate::ui::cas_panel::CasPanelState::default(),
            status: StatusBarState::default(),
            fps_meter: FpsMeter::default(),
            hint_elapsed: 0.0,
            hint_seen: None,
            camera: Camera::default(),
            orbit,
            fly: None,
            fly_keys: FlyKeys::default(),
            capture_active: false,
            suppress_capture_click: false,
            presets: PresetPanel::new(),
            style: settings.style,
            layout: settings.layout,
            layout_mode: LayoutMode::Wide,
            sheet: SheetState::None,
            sheet_expanded: SheetSize::Default,
            panel_generation: 0,
            appearance_open: false,
            about_open: false,
            help_open: false,
            last_viewport_rect: None,
            scene: ViewportScene::Vector(SceneGPU {
                arrows: Arc::new(Vec::new()),
                lines: Arc::new(backdrop),
                trails: Arc::new(Vec::new()),
                dots: Arc::new(Vec::new()),
                combined: Arc::new(Vec::new()),
                arrow_style,
                arrow_style_2d,
                arrow_params,
                mode_3d,
            }),
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
            click_seeds: Vec::new(),
            last_click_seed: None,
            flow_lines: Vec::new(),
            click_lines: Vec::new(),
            ode_lines: Arc::new(Vec::new()),
            last_ode_epoch,
            ode_autofit_pending: false,
            screenshot: Screenshot::new(),
            svg_export_provenance: None,
            scene_frame,
            particle_available,
            last_field_mode,
        }
    }
}

impl Default for VfApp {
    fn default() -> Self {
        Self::new()
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
    let mut lines = match frame {
        Some(frame) => reference_plane_lines(&frame, show_grid, show_coordinate_system),
        None => reference_lines_visible(domain, sampling_3d, show_grid, show_coordinate_system),
    };
    lines.extend_from_slice(flow);
    lines.extend_from_slice(clicks);
    lines
}
