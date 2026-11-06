use vector_field_explorer::VfApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Vector Field Explorer",
        options,
        Box::new(|_cc| Ok(Box::new(VfApp::default()))),
    )
}
