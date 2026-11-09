pub mod app;
pub mod arrow_geometry;
pub mod arrow_style;
pub mod camera;
pub mod colormap;
pub mod domain;
pub mod export;
pub mod field;
pub mod fly_camera;
pub mod i18n;
pub mod particles;
pub mod persistence;
pub mod plane;
pub mod pointer_lock;
pub mod presets;
pub mod reference;
pub mod render;
pub mod renderer;
pub mod sampling;
pub mod screenshot;
pub mod sim_clock;
pub mod streamlines;
pub mod ui;
pub mod version;
pub mod view_command;
pub mod viewmodel;
pub mod window;

// Re-export key types for tests and external use
pub use app::VfApp;
pub use colormap::Colormap;
pub use domain::Domain;
pub use export::Preset;
#[cfg(not(target_arch = "wasm32"))]
pub use export::{load_presets, save_preset};
pub use field::FieldExpr;
pub use render::Camera;
