pub mod app;
pub mod colormap;
pub mod domain;
pub mod export;
pub mod field;
pub mod i18n;
pub mod presets;
pub mod render;
pub mod ui;

// Re-export key types for tests and external use
pub use app::VfApp;
pub use colormap::Colormap;
pub use domain::Domain;
pub use export::{Preset, load_presets, save_preset};
pub use field::FieldExpr;
pub use render::Camera;
