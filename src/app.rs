use std::sync::Arc;

use eframe::egui;
use glam::{Vec2, Vec3};

use crate::camera::OrbitCamera;
use crate::render::Camera;
use crate::renderer::{DrawCallback, DrawState, LineInstanceGPU, SceneGPU};
use crate::streamlines::click_seed;
use crate::ui::formula_bar::FormulaBarState;
use crate::ui::presets::{PresetPanel, default_preset_dir};
use crate::ui::sidebar::Controls;
use crate::ui::style_panel::{self, AppStyle};
use crate::viewmodel::{State, apply_camera_input};

// Particle cloud (ticket 05 / plan task 10): the viewmodel owns the toggle,
// the app owns the cloud and decides when to run these pure functions.
use crate::particles::{
    CLICK_BURST_PARTICLES,
    Particles,
    DOT_SPACING_FRACTION,
    advect,
    clear,
    dot_instances,
    spawn,
    spawn_at,
    target_count,
    count as particle_count,
};

/// Directory where the app persists camera presets. Kept in one place so the
/// panel and any export helper always agree (preset-persistence plan Task 3).
pub fn presets_data_dir() -> std::path::PathBuf {
    default_preset_dir()
}

/// What the status bar reports this frame: the latest formula parse state
/// (empty when the field is good) and the size of the scene being painted —
/// field geometry only (arrows + streamline segments + dots; the app's
/// backdrop lines are decoration and are excluded from the count).
pub struct StatusBarState {
    pub parse_message: String,
    pub instance_count: usize,
    pub streamline_count: usize,
}

/// Radians of orbit per point of left-drag on the viewport.
const ORBIT_SENSITIVITY: f32 = 0.005;

/// How many viewport clicks keep their seeded bursts. Clicks accumulate only up
/// to this bound (the oldest burst is dropped), so repeated clicking cannot
/// grow the scene without limit.
const MAX_CLICK_SEEDS: usize = 6;

/// Application root state: the viewmodel State (the single source of truth for
/// what is on screen), the formula bar's local widgets, the sidebar's local
/// Controls, the status readouts, both cameras (the 2D `Camera` and the
/// `OrbitCamera`), the preset manager, the persisted UI-chrome style and the
/// GPU scene currently being painted.
pub struct VfApp {
    pub state: State,
    pub bar: FormulaBarState,
    pub sidebar: Controls,
    pub status: StatusBarState,
    pub camera: Camera,
    pub orbit: OrbitCamera,
    pub presets: PresetPanel,
    /// The persisted look-and-feel record (ticket 07): restored from eframe's
    /// app-state storage on startup and written back in [`eframe::App::save`].
    /// It lives app-side and never touches the viewmodel `State`.
    pub style: AppStyle,
    scene: SceneGPU,
    renderer_ready: bool,
    streamline_count: usize,
    /// The advected cloud behind the "Particles" toggle (ticket 05): 3D-view
    /// only; its per-frame dots ramp with `dots_range`, the same magnitude
    /// range the arrows were coloured with at the last scene rebuild.
    particles: Particles,
    dots_range: (f32, f32),
    /// Where the user clicked in the viewport (ticket 06), bounded by
    /// [`MAX_CLICK_SEEDS`] and oldest-first. The viewmodel turns each pick into
    /// a deterministic streamline burst; the newest click also drops a
    /// particle burst when the particle toggle is on.
    click_seeds: Vec<Vec3>,
    /// The grid-seeded streamline segments from the last scene rebuild, kept
    /// separately so a click can recompose the line collection without
    /// re-sampling or re-tracing the grid.
    flow_lines: Vec<LineInstanceGPU>,
    /// The current click bursts' streamline segments.
    click_lines: Vec<LineInstanceGPU>,
}

impl VfApp {
    /// App with no persistent storage (defaults for the style record).
    pub fn new() -> Self {
        Self::new_with_storage(None)
    }

    /// App wired to eframe's persistent storage: the style record is restored
    /// from it (absent/corrupt values fall back to the defaults) and written
    /// back on save.
    pub fn new_with_storage(storage: Option<&dyn eframe::Storage>) -> Self {
        let state = State::new(false);
        Self {
            state,
            bar: FormulaBarState::new(),
            sidebar: Controls::new(),
            status: StatusBarState {
                parse_message: String::new(),
                instance_count: 0,
                streamline_count: 0,
            },
            camera: Camera::default(),
            orbit: OrbitCamera::default(),
            presets: PresetPanel::new_with_dir(presets_data_dir())
                .unwrap_or_else(|_| PresetPanel::new()),
            style: style_panel::load(storage),
            scene: SceneGPU {
                arrows: Arc::new(Vec::new()),
                lines: Arc::new(reference_lines()),
                dots: Arc::new(Vec::new()),
            },
            renderer_ready: false,
            streamline_count: 0,
            particles: Particles::default(),
            dots_range: (0.0, 1.0),
            click_seeds: Vec::new(),
            flow_lines: Vec::new(),
            click_lines: Vec::new(),
        }
    }
}

impl Default for VfApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Grid + axes backdrop on the z = 0 plane (thin unlit lines). App-side
/// decoration: the viewmodel snapshot only owns the sampled field geometry.
fn reference_lines() -> Vec<LineInstanceGPU> {
    let mut lines = Vec::new();
    let half = 6f32;
    for i in -10..=10 {
        let t = i as f32 / 10.0 * half;
        let gray = if i == 0 { 0.45 } else { 0.22 };
        let axis_color = if i == 0 { [0.55, 0.2, 0.2, 1.0] } else { [gray, gray, gray + 0.03, 1.0] };
        lines.push(LineInstanceGPU {
            a: [t, -half, 0.0, 1.0],
            b: [t, half, 0.005, 0.015],
            color: axis_color,
        });
    }
    for i in -10..=10 {
        let t = i as f32 / 10.0 * half;
        let gray = if i == 0 { 0.45 } else { 0.22 };
        let axis_color = if i == 0 { [0.2, 0.55, 0.2, 1.0] } else { [gray, gray + 0.03, gray, 1.0] };
        lines.push(LineInstanceGPU {
            a: [-half, t, 0.0, 1.0],
            b: [half, t, 0.005, 0.015],
            color: axis_color,
        });
    }
    lines
}

/// Fold the backdrop, the grid-seeded streamlines and the click bursts into the
/// one line collection the viewport paints. Rebuilt only on a scene rebuild or
/// a click, so the per-frame path still clones an `Arc`.
fn compose_lines(flow: &[LineInstanceGPU], clicks: &[LineInstanceGPU]) -> Vec<LineInstanceGPU> {
    let mut lines = reference_lines();
    lines.extend_from_slice(flow);
    lines.extend_from_slice(clicks);
    lines
}

impl eframe::App for VfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Restyle the whole frame from the persisted record before any widget
        // runs. `apply` is idempotent, so doing this every frame is cheap and
        // keeps the UI in sync even if a theme/visual was changed elsewhere.
        style_panel::apply(ui.ctx(), &self.style);

        // Top formula bar: live edits, the field-preset ComboBox and the
        // 2D/3D toggle. Every edit goes through a viewmodel setter, which owns
        // parsing, clamping and the dirty flag.
        egui::Panel::top("formula_bar").show(ui, |ui| {
            crate::ui::formula_bar::show(ui, &mut self.state, &mut self.bar);
        });

        // Left sidebar: every scene knob, handed to the viewmodel setters
        // once per frame (the setters own clamping and the dirty flag). Shown
        // before the rebuild below so a control edit is reflected in this
        // frame's snapshot and in the status-bar count.
        egui::Panel::left("sidebar")
            .resizable(true)
            .min_size(220.0)
            .max_size(420.0)
            .show(ui, |ui| {
                // Style chrome lives at the top: it is UI config, not a field
                // knob, so it does not route through the viewmodel. A change
                // re-applies immediately so the rest of this frame restyles.
                if crate::ui::style_panel::show(ui, &mut self.style) {
                    crate::ui::style_panel::apply(ui.ctx(), &self.style);
                }
                crate::ui::sidebar::show(ui, &mut self.state, &mut self.sidebar);
            });

        // Rebuild the scene only when a setter marked the state dirty. The
        // app keeps owning the backdrop lines: fresh snapshots arrive on top
        // of a rebuilt grid + axes, and the streamline segments the viewmodel
        // seam emitted join them.
        if let Some(fresh) = self.state.snapshot_if_dirty() {
            self.scene.arrows = fresh.arrows;
            self.scene.dots = fresh.dots;
            // A rebuild refreshes the magnitude range the per-frame dots ramp
            // with (one extra sample pass on rebuild frames only), so dots
            // keep matching the arrows' colours.
            self.dots_range = self.state.color_range(&self.state.sample());
            self.flow_lines = Arc::unwrap_or_clone(fresh.lines);
            // Click bursts outlive the rebuild: they are anchored on the field,
            // not on the sampling grid, so retrace them against the new
            // domain/formula rather than dropping the user's picks.
            self.click_lines = self.state.click_streamlines_snapshot(self.dots_range, &self.click_seeds);
            self.streamline_count = self.flow_lines.len() + self.click_lines.len();
            self.scene.lines = Arc::new(compose_lines(&self.flow_lines, &self.click_lines));
        }

        // Particle cloud: while the toggle is on (3D view only — the 2D
        // camera has no depth for the dots to sit in), keep the cloud alive,
        // advect it by the frame's stable dt and rebuild the dot instances so
        // they ride the same color_range as the arrows. Placed after the
        // dirty-rebuild block so this frame's dots already see a fresh
        // density/speed, and before the status bar so its count includes them.
        if self.state.particles && self.state.mode_3d {
            if particle_count(&self.particles) == 0 {
                spawn(&mut self.particles, &self.state.domain, target_count(self.state.density), 3);
            }
            let dt = ui.ctx().input(|i| i.stable_dt).max(0.0);
            advect(&mut self.particles, &self.state.field, &self.state.domain, dt, self.state.speed);
            self.scene.dots = Arc::new(
                dot_instances(
                    &self.particles,
                    &self.state.field,
                    &self.state.domain,
                    self.state.colormap,
                    self.dots_range,
                    DOT_SPACING_FRACTION * self.state.spacing(),
                ),
            );
        } else if particle_count(&self.particles) > 0 {
            // Toggle off (or the 2D view): drop the cloud so switching back
            // spawns fresh at the fixed seed; the dots were already emptied
            // because the toggle marks the state dirty.
            clear(&mut self.particles);
        }

        // Bottom status bar: the parse state (red when bad) plus how many
        // field-geometry instances are being painted — arrows, streamline
        // segments and dots; the app's backdrop lines are decoration and
        // stay out of the count, while the streamline portion gets its own
        // readout so toggling streamlines moves a visible number.
        self.status.parse_message = self.state.parse_error.clone();
        self.status.instance_count =
            self.scene.arrows.len() + self.streamline_count + self.scene.dots.len();
        self.status.streamline_count = self.streamline_count;
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            crate::ui::status_bar::show(ui, &self.status);
        });

        // Presets panel on the right.
        egui::Panel::right("presets")
            .resizable(true)
            .min_size(200.0)
            .max_size(400.0)
            .show(ui, |ui| {
                self.presets.show(ui, &mut self.camera);
            });

        // Central viewport: the wgpu field renderer, driven by the same state.
        egui::CentralPanel::default_margins().show(ui, |ui| {
            let rect = ui.max_rect().shrink(2.0);
            let Some(render_state) = frame.wgpu_render_state() else {
                ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(30, 33, 41));
                return;
            };

            if !self.renderer_ready {
                let mut renderer = render_state.renderer.write();
                renderer
                    .callback_resources
                    .insert(DrawState::new(&render_state.device, render_state.target_format));
                self.renderer_ready = true;
            }

            // Camera gestures feed the seam's pure camera action: a left-drag
            // orbits in 3D mode only (2D keeps its fixed front view), and a
            // ctrl-scroll or pinch zoom factor zooms in both (the camera clamps
            // it). The response also senses clicks, so a press+release without
            // dragging seeds the flow (egui only reports `clicked` when the
            // gesture never became a drag, so orbiting/panning is untouched).
            let resp = ui.allocate_response(rect.size(), egui::Sense::click_and_drag());
            let drag = resp.drag_delta() * ORBIT_SENSITIVITY;
            let zoom = ui.ctx().input(|i| i.zoom_delta());
            self.orbit = apply_camera_input(self.orbit, self.state.mode_3d, drag.x, drag.y, zoom);

            let width = rect.width().max(1.0);
            let height = rect.height().max(1.0);
            let aspect = width / height;

            // Click-to-seed: the pure entry point turns the pointer position
            // into a point on the camera ray (3D, at the focal plane) or on the
            // 2D plane, then the viewmodel retraces the (bounded) burst list.
            if resp.clicked()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let ndc = Vec2::new(
                    (pos.x - rect.left()) / width * 2.0 - 1.0,
                    1.0 - (pos.y - rect.top()) / height * 2.0,
                );
                let ortho = if self.state.mode_3d { None } else { Some(self.state.plane_size()) };
                if let Some(seed) = click_seed(self.orbit, &self.state.domain, aspect, ndc, ortho) {
                    if self.click_seeds.len() >= MAX_CLICK_SEEDS {
                        self.click_seeds.remove(0);
                    }
                    self.click_seeds.push(seed);
                    self.click_lines = self.state.click_streamlines_snapshot(self.dots_range, &self.click_seeds);
                    self.streamline_count = self.flow_lines.len() + self.click_lines.len();
                    self.scene.lines = Arc::new(compose_lines(&self.flow_lines, &self.click_lines));
                    // Particles only exist in the 3D view; a click adds a
                    // capped burst around the pick (the per-frame dot
                    // rebuild picks it up on the next frame). If the cloud
                    // has not spawned yet, fill it first so the click does
                    // not leave a cloud of just this burst.
                    if self.state.particles && self.state.mode_3d {
                        if particle_count(&self.particles) == 0 {
                            spawn(
                                &mut self.particles,
                                &self.state.domain,
                                target_count(self.state.density),
                                3,
                            );
                        }
                        spawn_at(
                            &mut self.particles,
                            &self.state.domain,
                            seed,
                            CLICK_BURST_PARTICLES,
                            3,
                        );
                    }
                }
            }

            // Painter's algorithm: the wgpu pass has no depth attachment, so
            // order the instances far-to-near along this frame's view.
            let view = self.orbit.view();
            let mut arrows = Arc::unwrap_or_clone(Arc::clone(&self.scene.arrows));
            crate::renderer::sort_back_to_front(arrows.as_mut_slice(), &view);
            self.scene.arrows = Arc::new(arrows);

            let scene = SceneGPU {
                arrows: Arc::clone(&self.scene.arrows),
                lines: Arc::clone(&self.scene.lines),
                dots: Arc::clone(&self.scene.dots),
            };

            // 2D mode covers the domain's plane orthographically; 3D mode is
            // the orbiting perspective camera.
            let view_proj = self.orbit.vp_matrix(
                aspect,
                if self.state.mode_3d { None } else { Some(self.state.plane_size()) },
            );

            let callback = eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                DrawCallback {
                    scene,
                    view_proj,
                    camera_pos: self.orbit.position(),
                },
            );
            ui.painter().add(callback);
        });
    }

    /// Persist the UI-chrome style record under eframe's app key. Deliberately
    /// narrow (ticket 07 ruling): only the style record is stored here, so a
    /// later settings envelope can extend it without touching this contract.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        style_panel::save(storage, &self.style);
    }
}
