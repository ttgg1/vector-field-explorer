//! Left sidebar: every scene knob the widgets expose — sampling density,
//! arrow length and normalization, colormap and color range, the Features
//! stubs (streamlines, particles, speed) and the six domain extrema.
//!
//! The widgets edit a sidebar-local mirror ([`Controls`], like the formula
//! bar's [`FormulaBarState`]); `show` hands those values to the viewmodel
//! setters once per frame, and the setters own clamping, the min<max swaps
//! and the dirty flag. After applying them the mirror is refreshed from the
//! state, so the next frame's widgets start from the effective values: a
//! density the mode clamped down, extrema the seam swapped.
//!
//! Ruling (ledger): the density clamp keys to the dimension BEING SAMPLED
//! — 4..=200 while sampling runs flat, 4..=40 only when sampling runs
//! n³ — so the widget's range keys to `State::is_3d()` and reuses the
//! seam's own constants rather than re-inventing bounds. The seam still
//! clamps; the UI only stays consistent with it.

use eframe::egui;

use crate::colormap::Colormap;
use crate::domain::Domain;
use crate::viewmodel::{
    State,
    DEFAULT_DENSITY_2D,
    MIN_DENSITY,
    MAX_DENSITY_2D,
    MAX_DENSITY_3D,
    MIN_ARROW_SCALE,
    MAX_ARROW_SCALE,
    MIN_SPEED,
    MAX_SPEED,
};
use glam::Vec3;

/// The knobs the widgets edit between frames. Starts at the same defaults as
/// a fresh 2D [`State`] so an untouched first sidebar frame never marks the
/// scene dirty (and the status bar counts the scene the user actually sees).
#[derive(Debug, Clone)]
pub struct Controls {
    pub density: u32,
    pub arrow_scale: f32,
    pub normalize: bool,
    pub colormap: Colormap,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    pub streamlines: bool,
    pub particles: bool,
    pub speed: f32,
    pub domain_min: Vec3,
    pub domain_max: Vec3,
}

impl Controls {
    /// The knobs at the viewmodel defaults: density 24 (the 2D default — the
    /// mode toggle re-instances anyway), arrow scale 1, viridis, auto-fit
    /// range, traces off, the default domain box.
    pub fn new() -> Controls {
        let domain = Domain::default();
        Controls {
            density: DEFAULT_DENSITY_2D,
            arrow_scale: 1.0,
            normalize: false,
            colormap: Colormap::Viridis,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            streamlines: false,
            particles: false,
            speed: 1.0,
            domain_min: domain.min,
            domain_max: domain.max,
        }
    }
}

/// The same defaults as [`Controls::new`], so the type honours clippy's
/// `new_without_default`: the app can reach its starting knobs either way.
impl Default for Controls {
    fn default() -> Controls { Controls::new() }
}

/// Draw the sidebar and hand every edit to the viewmodel setters.
pub fn show(ui: &mut egui::Ui, state: &mut State, controls: &mut Controls) {
    // The density ceiling keys to the dimension being sampled (the ledger
    // ruling): n³ only when the mode is 3D and the field carries z.
    let sampling_3d = state.is_3d();
    let max_density = if sampling_3d { MAX_DENSITY_3D } else { MAX_DENSITY_2D };

    egui::CollapsingHeader::new("Display").default_open(true).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(if sampling_3d { "Density (n³)" } else { "Density (n²)" });
            ui.add(egui::DragValue::new(&mut controls.density).range(MIN_DENSITY..=max_density));
        });
        ui.add(
            egui::Slider::new(&mut controls.arrow_scale, MIN_ARROW_SCALE..=MAX_ARROW_SCALE)
                .text("Arrow length (grid cells)"),
        );
        ui.checkbox(&mut controls.normalize, "Normalize lengths");
    });

    egui::CollapsingHeader::new("Coloring").default_open(true).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Colormap");
            for colormap in Colormap::ALL {
                ui.selectable_value(&mut controls.colormap, colormap, colormap.label());
            }
        });
        ui.checkbox(&mut controls.auto_fit, "Auto-fit color range");
        if !controls.auto_fit {
            ui.horizontal(|ui| {
                ui.label("Magnitude range:");
                ui.add(egui::DragValue::new(&mut controls.color_min).speed(0.01));
                ui.label("to");
                ui.add(egui::DragValue::new(&mut controls.color_max).speed(0.01));
            });
        }
    });

    // Features stays a stub until tickets 04/05 land streamlines and
    // particles: the toggles mark the scene dirty but draw nothing yet.
    egui::CollapsingHeader::new("Features").show(ui, |ui| {
        ui.checkbox(&mut controls.streamlines, "Streamlines");
        ui.checkbox(&mut controls.particles, "Particles");
        ui.add(
            egui::Slider::new(&mut controls.speed, MIN_SPEED..=MAX_SPEED).text("Trace speed"),
        );
    });

    // Six extrema; reversed drags are handed over as-is and the seam's
    // per-axis swap normalizes them (mirrored back below so the widgets
    // show the effective box).
    egui::CollapsingHeader::new("Domain").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("x:");
            ui.add(egui::DragValue::new(&mut controls.domain_min.x).speed(0.01));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut controls.domain_max.x).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("y:");
            ui.add(egui::DragValue::new(&mut controls.domain_min.y).speed(0.01));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut controls.domain_max.y).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("z:");
            ui.add(egui::DragValue::new(&mut controls.domain_min.z).speed(0.01));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut controls.domain_max.z).speed(0.01));
        });
    });

    // Hand the mirror to the seam; each setter that actually changes
    // something marks the state dirty, which feeds the snapshot and the
    // status-bar instance count rebuilt right after this panel in app.rs.
    state.set_density(controls.density);
    state.set_arrow_scale(controls.arrow_scale);
    state.set_normalize(controls.normalize);
    state.set_colormap(controls.colormap);
    state.set_auto_fit(controls.auto_fit);
    state.set_color_range(controls.color_min, controls.color_max);
    state.set_streamlines(controls.streamlines);
    state.set_particles(controls.particles);
    state.set_speed(controls.speed);
    state.set_domain(controls.domain_min, controls.domain_max);

    // Refresh the mirror from the effective state so next frame starts
    // from truth (clamped density for this mode, swapped extrema, …).
    controls.density = state.density;
    controls.arrow_scale = state.arrow_scale;
    controls.normalize = state.normalize;
    controls.colormap = state.colormap;
    controls.auto_fit = state.auto_fit;
    controls.color_min = state.color_min;
    controls.color_max = state.color_max;
    controls.streamlines = state.streamlines;
    controls.particles = state.particles;
    controls.speed = state.speed;
    controls.domain_min = state.domain.min;
    controls.domain_max = state.domain.max;
}
