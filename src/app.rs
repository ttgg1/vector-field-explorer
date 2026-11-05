use eframe::egui;

use crate::ui;

/// Stub control knobs registered from the sidebar. Behavior arrives in later tasks.
#[derive(Debug, Clone)]
pub struct Controls {
    pub density: u32,
    pub arrow_scale: f32,
    pub normalize: bool,
    /// Placeholder colormap field (stringly typed for now).
    pub colormap: String,
    pub color_range: (f32, f32),
    pub extent: Extent,
    pub features: Features,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            density: 24,
            arrow_scale: 1.0,
            normalize: true,
            colormap: "viridis".to_owned(),
            color_range: (-1.0, 1.0),
            extent: Extent::default(),
            features: Features::default(),
        }
    }
}

/// 2D plotting domain of the field.
#[derive(Debug, Clone)]
pub struct Extent {
    pub x_min: f32,
    pub x_max: f32,
    pub y_min: f32,
    pub y_max: f32,
}

impl Default for Extent {
    fn default() -> Self {
        Self {
            x_min: -2.0,
            x_max: 2.0,
            y_min: -2.0,
            y_max: 2.0,
        }
    }
}

/// Feature toggles for later drawing tasks. All stubs for now.
#[derive(Debug, Clone)]
pub struct Features {
    pub arrows: bool,
    pub streamlines: bool,
    pub equipotentials: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            arrows: true,
            streamlines: false,
            equipotentials: false,
        }
    }
}

/// Bottom status bar state.
#[derive(Debug, Clone, Default)]
pub struct StatusBarState {
    pub parse_message: String,
    pub instance_count: usize,
}

/// Application root state. Layout only — data wiring comes later.
#[derive(Debug, Clone)]
pub struct VfApp {
    pub formula: String,
    /// false = 2D mode, true = 3D mode.
    pub mode3d: bool,
    pub controls: Controls,
    pub status: StatusBarState,
    pub presets: ui::presets::PresetPanel,
    pub camera: render::Camera,
    pub camera: render::Camera,
    pub presets: ui::presets::PresetPanel,
    pub formula: String,
    /// false = 2D mode, true = 3D mode.
    pub mode3d: bool,
    pub controls: Controls,
    pub status: StatusBarState,
}

impl VfApp {
    pub fn new() -> Self {
        let mut app = Self {
            formula: "sin(x) * cos(y)".to_owned(),
            mode3d: false,
            controls: Controls::default(),
            status: StatusBarState {
                parse_message: "OK".to_owned(),
                instance_count: 0,
            },
            presets: ui::presets::PresetPanel::new(),
            camera: render::Camera::default(),
        };
        app
            formula: "sin(x) * cos(y)".to_owned(),
            mode3d: false,
            controls: Controls::default(),
            status: StatusBarState {
                parse_message: "OK".to_owned(),
                instance_count: 0,
            },
            presets: ui::presets::PresetPanel::new(),
        };
        app

        Self {
            formula: "sin(x) * cos(y)".to_owned(),
            mode3d: false,
            controls: Controls::default(),
            status: StatusBarState {
                parse_message: "OK".to_owned(),
                instance_count: 0,
            },
        }
    }
}

impl Default for VfApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for VfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Top formula bar.
        egui::Panel::top("formula_bar").show(ui, |ui| {
            ui::formula_bar::show(
                ui,
                &mut self.formula,
                &mut self.mode3d,
                &mut self.status,
            );
        });

        // Bottom status bar.
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui::status_bar::show(ui, &mut self.status);
        });

        // Resizable left sidebar.
        egui::Panel::left("sidebar")
            .resizable(true)
            .min_size(180.0)
            .max_size(480.0)
            .show(ui, |ui| {
                ui::sidebar::show(ui, &mut self.controls);
        // Export button
        if ui.button("Export PNG").clicked() {
            let width = 800;
            let height = 600;
            let frame: Vec<u8> = vec![0; (width * height * 4) as usize];
            if let Err(e) = crate::export::save_png(&frame, width, height, "screenshot.png") {
                eprintln!("Export failed: {:?}", e);
            }
        }
    });

        // Presets panel
        egui::Panel::right("presets")
            .resizable(true)
            .min_size(200.0)
            .max_size(400.0)
            .show(ui, |ui| {
                self.presets.show(ui, &mut self.camera);
            });
        if ui.button("Export PNG").clicked() {
            // Dummy frame data for demo; in real app would capture GPU output.
            let width = 800;
            let height = 600;
            let frame: Vec<u8> = vec![0; (width * height * 4) as usize];
            if let Err(e) = crate::export::save_png(&frame, width, height, "screenshot.png") {
                eprintln!("Export failed: {:?}", e);
            }
        }
            });

        // Central viewport placeholder (painter rect; wgpu callback arrives later).
        egui::CentralPanel::default_margins().show(ui, |ui| {
            let rect = ui.max_rect().shrink(2.0);
            ui.painter()
                .rect_filled(rect, 8.0, egui::Color32::from_rgb(30, 33, 41));
            ui.centered_and_justified(|ui| {
                ui.weak("placeholder — wgpu field rendering arrives in a later task");
            });
        });
    }
}
