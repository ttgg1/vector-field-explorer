use std::sync::Arc;

use eframe::egui;

use crate::camera::OrbitCamera;
use crate::render::Camera;
use crate::renderer::{DrawCallback, DrawState, LineInstanceGPU, SceneGPU};
use crate::ui::formula_bar::FormulaBarState;
use crate::ui::presets::{PresetPanel, default_preset_dir};
use crate::ui::sidebar::Controls;
use crate::viewmodel::{State, apply_camera_input};

/// Directory where the app persists camera presets. Kept in one place so the
/// panel and any export helper always agree (preset-persistence plan Task 3).
pub fn presets_data_dir() -> std::path::PathBuf {
    default_preset_dir()
}

/// What the status bar reports this frame: the latest formula parse state
/// (empty when the field is good) and the size of the scene being painted.
pub struct StatusBarState {
    pub parse_message: String,
    pub instance_count: usize,
}

/// Radians of orbit per point of left-drag on the viewport.
const ORBIT_SENSITIVITY: f32 = 0.005;

/// Application root state: the viewmodel (the single source of truth for what
/// is on screen), the formula bar's local widgets, the sidebar's local controls,
/// the status readouts, both cameras, the preset manager and the GPU scene
/// cameras, the preset manager and the GPU scene currently being painted.
pub struct VfApp {
    pub state: State,
    pub bar: FormulaBarState,
    pub sidebar: Controls,
    pub status: StatusBarState,
    pub camera: Camera,
    pub orbit: OrbitCamera,
    pub presets: PresetPanel,
    scene: SceneGPU,
    renderer_ready: bool,
}

impl VfApp {
    pub fn new() -> Self {
        let state = State::new(false);
        Self {
            state,
            bar: FormulaBarState::new(),
            sidebar: Controls::new(),
            status: StatusBarState {
                parse_message: String::new(),
                instance_count: 0,
            },
            camera: Camera::default(),
            orbit: OrbitCamera::default(),
            presets: PresetPanel::new_with_dir(presets_data_dir())
                .unwrap_or_else(|_| PresetPanel::new()),
            scene: SceneGPU {
                arrows: Arc::new(Vec::new()),
                lines: Arc::new(reference_lines()),
                dots: Arc::new(Vec::new()),
            },
            renderer_ready: false,
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

impl eframe::App for VfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
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
                crate::ui::sidebar::show(ui, &mut self.state, &mut self.sidebar);
            });

        // Rebuild the scene only when a setter marked the state dirty.
        if let Some(fresh) = self.state.snapshot_if_dirty() {
            self.scene.arrows = fresh.arrows;
            self.scene.dots = fresh.dots;
        }

        // Bottom status bar: the parse state (red when bad) plus how many
        // arrow instances the scene currently being painted holds.
        self.status.parse_message = self.state.parse_error.clone();
        self.status.instance_count = self.scene.arrows.len();
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
            // it).
            let resp = ui.allocate_response(rect.size(), egui::Sense::drag());
            let drag = resp.drag_delta() * ORBIT_SENSITIVITY;
            let zoom = ui.ctx().input(|i| i.zoom_delta());
            self.orbit = apply_camera_input(self.orbit, self.state.mode_3d, drag.x, drag.y, zoom);

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
            let aspect = rect.width() / rect.height().max(1.0);
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
}
