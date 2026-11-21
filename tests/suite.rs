//! Single integration-test harness (ticket 21).
//!
//! Every `tests/*_test.rs` file remains in place with its tests and helpers
//! unchanged; this crate includes each one as a module so the whole suite runs
//! in one process. Test filters still work: `cargo test particles_test::` or
//! any test-name substring selects the same tests as before.

// Declared once; the test modules below import it with `use crate::support;`.
#[path = "support/mod.rs"]
mod support;

mod accent_picker_test;
mod animation_dialog_test;
#[cfg(not(target_arch = "wasm32"))]
mod animation_ffmpeg_test;
mod animation_gif_test;
mod animation_job_test;
mod animation_scene_test;
mod animation_test;
mod appearance_panel_test;
mod appearance_profile_test;
mod arrow_alpha_test;
mod arrow_geometry_test;
mod arrow_layer_style_test;
mod arrow_opacity_test;
mod arrow_style_test;
mod branding_assets_test;
mod button_sizing_test;
mod camera_input_test;
mod camera_pan_zoom_test;
mod camera_test;
mod camera_zoom_pan_test;
mod cas_actions_test;
#[cfg(all(unix, not(target_arch = "wasm32")))]
mod cas_backend_test;
mod cas_intent_test;
#[cfg(not(target_arch = "wasm32"))]
mod cas_panel_test;
#[cfg(not(target_arch = "wasm32"))]
mod cas_rails_test;
#[cfg(not(target_arch = "wasm32"))]
mod cas_test;
#[cfg(not(target_arch = "wasm32"))]
mod cas_tex_test;
#[cfg(unix)]
mod clean_test_artifacts_test;
#[cfg(not(target_arch = "wasm32"))]
mod cli_animation_test;
#[cfg(not(target_arch = "wasm32"))]
mod cli_gpu_test;
#[cfg(not(target_arch = "wasm32"))]
mod cli_scene_test;
#[cfg(not(target_arch = "wasm32"))]
mod cli_svg_test;
#[cfg(not(target_arch = "wasm32"))]
mod cli_test;
mod click_seed_zoom_test;
mod color_mapping_test;
mod color_ramp_test;
mod colormap_test;
mod domain_test;
mod dot_billboard_test;
mod download_link_test;
mod edit_history_app_test;
mod edit_history_test;
mod export_test;
mod field_probe_test;
mod field_test;
mod fly_camera_test;
mod fonts_test;
mod heatmap_gpu_test;
mod help_overlay_test;
mod induction_app_test;
mod induction_bench_test;
mod induction_export_test;
mod induction_panel_test;
mod induction_test;
mod interaction_panel_test;
mod interaction_routing_test;
mod interaction_test;
mod layer_style_test;
mod layout_test;
mod left_click_action_test;
mod line_alpha_test;
mod material_app_test;
mod material_bench_test;
mod material_export_test;
mod material_panel_test;
mod material_persistence_test;
mod material_test;
#[cfg(not(target_arch = "wasm32"))]
mod math_view_test;
mod maxwell_app_test;
mod maxwell_bem_test;
mod maxwell_bench_test;
mod maxwell_export_test;
mod maxwell_learning_test;
mod maxwell_panel_test;
mod maxwell_persistence_test;
mod maxwell_residual_test;
mod maxwell_test;
mod menu_test;
// POSIX shell scripts: `scripts/nightly-version.sh` and
// `scripts/generate-notices.sh` do not run on the Windows runner.
#[cfg(unix)]
mod nightly_version_test;
#[cfg(unix)]
mod notices_test;
mod object_scene_test;
mod ode_app_test;
mod ode_bench_test;
mod ode_export_test;
mod ode_panel_test;
mod ode_persistence_test;
mod ode_test;
mod packaging_notices_test;
mod palette_io_test;
mod panel_rails_test;
mod parameters_test;
mod particle_playback_test;
mod particles_test;
mod persistence_test;
mod plane_particles_test;
mod plane_scene_test;
mod plane_test;
mod plot_app_test;
mod plot_camera_test;
mod plot_curve_test;
mod plot_examples_test;
#[cfg(not(target_arch = "wasm32"))]
mod plot_gpu_test;
mod plot_renderer_test;
mod plot_state_test;
mod plot_surface_test;
mod plot_ui_test;
mod pointer_lock_test;
mod preset_library_test;
mod preset_test;
mod presets_test;
mod profile_library_test;
mod reference_lines_test;
mod responsive_test;
mod sampling_test;
mod scalar_analysis_test;
mod scalar_animation_app_test;
mod scalar_animation_test;
mod scalar_clock_test;
mod scalar_heatmap_test;
mod scalar_test;
mod scalar_view_test;
mod scene_document_test;
mod scene_flow_test;
mod scene_store_test;
mod screenshot_test;
mod seed_layout_library_test;
mod seed_layout_test;
mod shortcut_test;
mod sidebar_camera_ui_test;
mod sidebar_colormap_ui_test;
mod sidebar_controls_test;
mod sidebar_sections_test;
mod source_export_app_test;
mod source_export_test;
mod source_glyph_test;
mod source_mode_test;
mod source_preset_bench_test;
mod source_presets_test;
mod source_scene_test;
mod source_slice_test;
mod sources_electric_test;
mod sources_evaluate_test;
mod sources_magnetic_test;
mod sources_model_test;
mod sources_numeric_test;
mod sources_panel_test;
mod status_bar_test;
mod streamline_count_test;
mod streamlines_test;
mod svg_export_test;
mod test_layout_test;
mod top_bar_test;
mod ui_preset_test;
mod ui_style_panel_test;
mod version_test;
mod view_command_test;
mod viewmodel_test;
mod viewport_style_test;
mod web_assets_test;
mod window_test;
