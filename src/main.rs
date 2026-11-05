mod app;
mod field;
mod ui;

use app::VfApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Vector Field Explorer",
        native_options,
        Box::new(|_cc| Ok(Box::new(VfApp::new()))),
    )
}
