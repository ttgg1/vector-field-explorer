use std::sync::Arc;

use eframe::egui;
use glam::{Mat4, UVec3, Vec3};

use crate::camera::OrbitCamera;
use crate::domain::Domain;
use crate::field::FieldExpr;
use crate::render::Camera;
use crate::renderer::{
    ArrowInstanceGPU, DrawCallback, DrawState, LineInstanceGPU, SceneGPU,
};
use crate::sampling::{ViewOptions, sample_field};
use crate::ui::presets::{PresetPanel, default_preset_dir};

/// Directory where the app persists camera presets. Kept in one place so the
/// panel and any export helper always agree (preset-persistence plan Task 3).
pub fn presets_data_dir() -> std::path::PathBuf {
    default_preset_dir()
}

/// Arrow-grid resolution for the initial viewport (main plan cap: 200x200,
/// default 24; task 8 makes this a sidebar control).
const SAMPLE_GRID: u32 = 24;

/// Application root state: formula bar, camera viewport, preset manager.
pub struct VfApp {
    pub formula: String,
    pub parse_message: String,
    pub camera: Camera,
    pub orbit: OrbitCamera,
    pub presets: PresetPanel,
    field: Option<FieldExpr>,
    renderer_ready: bool,
}

impl VfApp {
    pub fn new() -> Self {
        Self {
            formula: "x, y".to_owned(),
            parse_message: "OK".to_owned(),
            camera: Camera::default(),
            orbit: OrbitCamera::default(),
            presets: PresetPanel::new_with_dir(presets_data_dir())
                .unwrap_or_else(|_| PresetPanel::new()),
            field: None,
            renderer_ready: false,
        }
    }
}

impl Default for VfApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Re-parses the formula, reporting the parse state. Keeps the last good
/// field when a new parse fails so the viewport stays up while editing (the
/// status bar shows the error instead).
fn refresh_field(app: &mut VfApp) {
    match FieldExpr::parse(&app.formula) {
        Ok(f) => {
            app.field = Some(f);
            app.parse_message = "OK".to_owned();
        }
        Err(e) => {
            app.parse_message = e.to_string();
        }
    }
}

/// Grid + axes backdrop on the z = 0 plane (thin unlit lines).
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

/// Builds the scene for the viewport: sampled arrows (sorted back-to-front),
/// reference grid, and the camera view-projection for this frame.
fn build_scene(app: &VfApp, aspect: f32) -> (SceneGPU, Mat4) {
    let field = app
        .field
        .as_ref()
        .expect("refresh_field must run before build_scene in the same frame");
    let domain = Domain::default();
    let view_options = ViewOptions::default();
    let mut sampled =
        sample_field(field, &domain, UVec3::new(SAMPLE_GRID, SAMPLE_GRID, 1), &view_options);

    let view = app.orbit.view();
    sort_back_to_front_sampled(&mut sampled, &view);

    let arrows: Vec<ArrowInstanceGPU> = sampled
        .iter()
        .map(|d| ArrowInstanceGPU {
            origin: [d.position.x, d.position.y, d.position.z, 1.0],
            dir_len: [d.dir.x, d.dir.y, d.dir.z, d.mag],
            color: [
                d.color[0] as f32 / 255.0,
                d.color[1] as f32 / 255.0,
                d.color[2] as f32 / 255.0,
                1.0,
            ],
        })
        .collect();

    let scene = SceneGPU {
        arrows: Arc::new(arrows),
        lines: Arc::new(reference_lines()),
        dots: Arc::new(Vec::new()),
    };
    // Broad enough to always cover the default [-1, 1] domain plus margins.
    let plane_size = 6.5;
    (scene, app.orbit.vp_matrix(aspect, Some(plane_size)))
}

/// Reorders sampled arrow data far-to-near along the view direction before
/// instance conversion (the renderer's `sort_back_to_front` works on
/// instances; this variant avoids needing instance key math twice).
fn sort_back_to_front_sampled(
    sampled: &mut [crate::sampling::ArrowData],
    view: &Mat4,
) {
    sampled.sort_by(|a, b| {
        let pa = (view * a.position.extend(1.0)).z;
        let pb = (view * b.position.extend(1.0)).z;
        pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
    });
}

impl eframe::App for VfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Top formula bar.
        egui::Panel::top("formula_bar").show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.formula)
                    .desired_width(f32::INFINITY),
            );
        });

        // Bottom status bar.
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.parse_message);
                ui.separator();
                ui.label(format!(
                    "camera at ({:.1}, {:.1}), scale {:.2}",
                    self.camera.position[0], self.camera.position[1], self.camera.scale
                ));
            });
        });

        // Presets panel on the right.
        egui::Panel::right("presets")
            .resizable(true)
            .min_size(200.0)
            .max_size(400.0)
            .show(ui, |ui| {
                self.presets.show(ui, &mut self.camera);
            });

        // Central viewport: the wgpu field renderer.
        egui::CentralPanel::default_margins().show(ui, |ui| {
            let rect = ui.max_rect().shrink(2.0);
            let Some(render_state) = frame.wgpu_render_state() else {
                ui.painter()
                    .rect_filled(rect, 8.0, egui::Color32::from_rgb(30, 33, 41));
                return;
            };

            if !self.renderer_ready {
                let mut renderer = render_state.renderer.write();
                renderer
                    .callback_resources
                    .insert(DrawState::new(&render_state.device, render_state.target_format));
                self.renderer_ready = true;
            }

            refresh_field(self);
            let aspect = rect.width() / rect.height().max(1.0);
            let (scene, view_proj) = build_scene(self, aspect);
            let camera_pos = self.orbit.position();

            let callback = eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                DrawCallback {
                    scene,
                    view_proj,
                    camera_pos: Vec3::new(camera_pos[0], camera_pos[1], camera_pos[2]),
                },
            );
            ui.painter().add(callback);
        });
    }
}
