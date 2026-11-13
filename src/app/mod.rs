use std::sync::Arc;

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
use crate::reference::reference_lines_visible;
use crate::render::Camera;
use crate::renderer::{
    DrawCallback, DrawState, LineInstanceGPU, SceneGPU, combined_instances,
    sort_scene_back_to_front, sorted_arrows_back_to_front, sorted_lines_back_to_front,
};
use crate::screenshot::{self, Instant, Screenshot};
use crate::sim_clock::SimClock;
use crate::streamlines::click_seed;
use crate::ui::formula_bar::FormulaBarState;
use crate::ui::layout::{self, LayoutMode, LayoutState, SheetSize, SheetState};
use crate::ui::menu::{self, MenuAction};
use crate::ui::presets::PresetPanel;
#[cfg(not(target_arch = "wasm32"))]
use crate::ui::presets::default_preset_dir;
use crate::ui::sidebar::Controls;
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
    scene: SceneGPU,
    renderer_ready: bool,
    streamline_count: usize,
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
    /// The screenshot action (ticket 08): the pending capture request, the
    /// channel its save thread reports on, and the completion toast.
    pub screenshot: Screenshot,
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
            orbit.recentre_on(&state.domain);
        } else {
            orbit.recentre_on_2d(&state.domain);
        }
        let backdrop = compose_lines(
            &[],
            &[],
            &state.domain,
            state.is_3d(),
            state.show_grid,
            state.show_coordinate_system,
        );
        let arrow_style = state.arrow_style;
        let arrow_style_2d = state.arrow_style_2d;
        let arrow_params = state.arrow_params;
        let mode_3d = state.mode_3d;
        Self {
            state,
            bar: FormulaBarState::new(),
            controls,
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
            scene: SceneGPU {
                arrows: Arc::new(Vec::new()),
                lines: Arc::new(backdrop),
                trails: Arc::new(Vec::new()),
                dots: Arc::new(Vec::new()),
                combined: Arc::new(Vec::new()),
                arrow_style,
                arrow_style_2d,
                arrow_params,
                mode_3d,
            },
            renderer_ready: false,
            streamline_count: 0,
            particles: Particles::default(),
            particle_target: None,
            clock: SimClock::default(),
            dots_range: (0.0, 1.0),
            click_seeds: Vec::new(),
            last_click_seed: None,
            flow_lines: Vec::new(),
            click_lines: Vec::new(),
            screenshot: Screenshot::new(),
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
    let mut lines = reference_lines_visible(domain, sampling_3d, show_grid, show_coordinate_system);
    lines.extend_from_slice(flow);
    lines.extend_from_slice(clicks);
    lines
}
