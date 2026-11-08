use vector_field_explorer::VfApp;
use vector_field_explorer::window::{WINDOW_TITLE, window_options};

fn main() -> eframe::Result {
    let options = window_options();
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(VfApp::new_with_storage(cc.storage)))),
    )
}
