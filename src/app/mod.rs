use std::sync::{Arc, Mutex};

use eframe::egui;
use glam::{Mat4, Vec2, Vec3};

use crate::animation::AnimationError;
use crate::animation_job::{AnimationEligibility, AnimationJob, AnimationJobState};
use crate::animation_scene::AnimationSnapshot;
use crate::camera::{OrbitCamera, ViewCamera, apply_camera_record};
use crate::color_mapping::{ClipCounts, ColorMapping, ColorSample, ColorStats};
use crate::domain::Domain;
use crate::fly_camera::{
    CROSSHAIR_ALPHA, FlyCamera, FlyKeys, capture_exit_required, capture_transition,
    crosshair_segments, crosshair_visible, exit_on_focus_loss, frame_look, is_command_chord,
    sync_fly, sync_fly_mirror,
};
use crate::persistence::{self, AppSettings, PresetUiRecord, ViewRecord};
use crate::plane::PlaneFrame;
use crate::plot::PlotKind;
use crate::plot_camera::PlotCamera;
use crate::plot_scene::{PlotScene, ViewportScene};
use crate::reference::{reference_lines_visible_with, reference_plane_lines_with};
use crate::render::Camera;
use crate::renderer::{
    ArrowInstanceGPU, DrawCallback, DrawState, LineInstanceGPU, SceneGPU, combined_instances,
    sort_scene_back_to_front, sorted_arrows_back_to_front, sorted_lines_back_to_front,
};
use crate::screenshot::{self, Instant, Screenshot};
use crate::sim_clock::SimClock;
use crate::source_scene::FieldMode;
use crate::streamlines::click_seed;
use crate::ui::animation_dialog::{self, AnimationDialogAction, AnimationDialogState};
use crate::ui::appearance_panel::AppearancePanelState;
use crate::ui::formula_bar::FormulaBarState;
use crate::ui::induction_panel::InductionPanelState;
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
    /// The right Appearance panel's inline error/readout state (issue #84
    /// slice A2b). The committed styles live in [`State::layer_styles`].
    pub appearance: AppearancePanelState,
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
        // Restoring the durable parameter set is not a parameter edit: assign
        // it directly so it never dirties the scene on its own. `load` already
        // replaced an invalid stored set with the default.
        state.parameters = settings.parameters;
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
        let mut orbit = OrbitCamera::default();
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
        // Restore the persisted group expansion before the first frame without
        // reporting a layout edit; the panel's search and filters stay fresh.
        let mut presets = PresetPanel::new();
        presets.restore_expanded(settings.preset_ui.expanded);
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
            presets,
            appearance: AppearancePanelState::default(),
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
        }
    }
}

impl Default for VfApp {
    fn default() -> Self {
        Self::new()
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
        ))
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
pub(crate) fn compose_vector_lines(
    state: &State,
    flow: &[LineInstanceGPU],
    clicks: &[LineInstanceGPU],
) -> (Vec<LineInstanceGPU>, usize) {
    let mut lines = compose_lines_with_frame_style(
        flow,
        clicks,
        &state.scene_domain(),
        state.is_3d(),
        state.plane_frame(),
        state.show_grid,
        state.show_coordinate_system,
        &state.reference_viewport_style(),
    );
    let glyphs = match state.field_mode {
        FieldMode::Sources => {
            let glyphs = crate::source_glyph::source_glyphs(
                &state.sources,
                &state.scene_domain(),
                state.plane_frame().as_ref(),
            );
            crate::source_glyph::source_glyph_lines(&glyphs, &state.scene_domain())
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
            crate::source_glyph::source_glyph_lines(&glyphs, &state.scene_domain())
        }
        FieldMode::Formula | FieldMode::Materials | FieldMode::Ode | FieldMode::Maxwell => {
            Vec::new()
        }
    };
    let count = glyphs.len();
    lines.extend(glyphs);
    (lines, count)
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
    if state.field_mode != FieldMode::Ode {
        cached.combined = Arc::clone(&combined);
    }
    SceneGPU {
        arrows,
        lines,
        trails,
        dots,
        combined,
        arrow_style,
        arrow_style_2d,
        arrow_params,
        mode_3d: state.mode_3d,
    }
}
