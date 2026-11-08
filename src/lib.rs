pub mod app;
pub mod camera;
pub mod colormap;
pub mod domain;
pub mod export;
pub mod field;
pub mod i18n;
pub mod particles;
pub mod persistence;
pub mod presets;
pub mod render;
pub mod renderer;
pub mod sampling;
pub mod screenshot;
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
pub use export::{Preset, load_presets, save_preset};
pub use field::FieldExpr;
pub use render::Camera;
