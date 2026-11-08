//! Left sidebar: every scene knob the widgets expose — sampling density,
//! arrow length and normalization, colormap and color range, streamlines and
//! particles with their trace speed, and the six domain extrema.
//!
//! Sections follow the agreed order (ticket 14): **Display → Features →
//! Coloring → Domain**. Display and Features start open, the two less-used
//! sections start collapsed, and every section has a reset-to-defaults button
//! in its header.
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

/// The Controls panel's sections, in display order (ticket 14 ruling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Display,
    Features,
    Coloring,
    Domain,
}

/// The sections in the order they appear in the panel.
pub const SECTIONS: [Section; 4] = [
    Section::Display,
    Section::Features,
    Section::Coloring,
    Section::Domain,
];

impl Section {
    /// Header text.
    pub fn title(self) -> &'static str {
        match self {
            Section::Display => "Display",
            Section::Features => "Features",
            Section::Coloring => "Coloring",
            Section::Domain => "Domain",
        }
    }

    /// Display and Features are the everyday controls, so they start open;
    /// Coloring and Domain keep their space until the user asks for them.
    pub fn default_open(self) -> bool {
        matches!(self, Section::Display | Section::Features)
    }
}

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

    /// Restore the Display section's defaults (density, arrow length,
    /// normalization).
    pub fn reset_display(&mut self) {
        let defaults = Controls::new();
        self.density = defaults.density;
        self.arrow_scale = defaults.arrow_scale;
        self.normalize = defaults.normalize;
    }

    /// Restore the Features section's defaults (traces off, speed 1).
    pub fn reset_features(&mut self) {
        let defaults = Controls::new();
        self.streamlines = defaults.streamlines;
        self.particles = defaults.particles;
        self.speed = defaults.speed;
    }

    /// Restore the Coloring section's defaults (viridis, auto-fit, 0..1).
    pub fn reset_coloring(&mut self) {
        let defaults = Controls::new();
        self.colormap = defaults.colormap;
        self.auto_fit = defaults.auto_fit;
        self.color_min = defaults.color_min;
        self.color_max = defaults.color_max;
    }

    /// Restore the Domain section's defaults (the unit cube).
    pub fn reset_domain(&mut self) {
        let defaults = Controls::new();
        self.domain_min = defaults.domain_min;
        self.domain_max = defaults.domain_max;
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

    section(ui, Section::Display, controls, |ui, controls| {
        ui.horizontal(|ui| {
            ui.label(if sampling_3d { "Density (n³)" } else { "Density (n²)" });
            ui.add(egui::DragValue::new(&mut controls.density).range(MIN_DENSITY..=max_density))
                .on_hover_text("Samples per axis; 3D sampling makes it the cube of this count");
        });
        ui.add(
            egui::Slider::new(&mut controls.arrow_scale, MIN_ARROW_SCALE..=MAX_ARROW_SCALE)
                .text("Arrow length (grid cells)"),
        )
        .on_hover_text("Arrow length as a multiple of one grid cell");
        ui.checkbox(&mut controls.normalize, "Normalize lengths")
            .on_hover_text("Scale arrows to unit length, keeping only their direction");
    });

    section(ui, Section::Features, controls, |ui, controls| {
        ui.checkbox(&mut controls.streamlines, "Streamlines")
            .on_hover_text("Trace field lines from a grid of seeds");
        ui.checkbox(&mut controls.particles, "Particles")
            .on_hover_text("Animate dots carried by the field (3D view)");
        ui.add(
            egui::Slider::new(&mut controls.speed, MIN_SPEED..=MAX_SPEED).text("Trace speed"),
        )
        .on_hover_text("How fast traces advance, in grid cells per second");
    });

    section(ui, Section::Coloring, controls, |ui, controls| {
        ui.horizontal(|ui| {
            ui.label("Colormap");
            for colormap in Colormap::ALL {
                ui.selectable_value(&mut controls.colormap, colormap, colormap.label())
                    .on_hover_text("Color map for field magnitude");
            }
        });
        ui.checkbox(&mut controls.auto_fit, "Auto-fit color range")
            .on_hover_text("Fit the color range to the sampled magnitudes");
        if !controls.auto_fit {
            ui.horizontal(|ui| {
                ui.label("Magnitude range:");
                ui.add(egui::DragValue::new(&mut controls.color_min).speed(0.01))
                    .on_hover_text("Magnitude at the color ramp's low end");
                ui.label("to");
                ui.add(egui::DragValue::new(&mut controls.color_max).speed(0.01))
                    .on_hover_text("Magnitude at the color ramp's high end");
            });
        }
    });

    // Six extrema; reversed drags are handed over as-is and the seam's
    // per-axis swap normalizes them (mirrored back below so the widgets
    // show the effective box).
    section(ui, Section::Domain, controls, |ui, controls| {
        ui.horizontal(|ui| {
            ui.label("x:");
            ui.add(egui::DragValue::new(&mut controls.domain_min.x).speed(0.01));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut controls.domain_max.x).speed(0.01));
        })
        .response
        .on_hover_text("Sampled x extent, world units");
        ui.horizontal(|ui| {
            ui.label("y:");
            ui.add(egui::DragValue::new(&mut controls.domain_min.y).speed(0.01));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut controls.domain_max.y).speed(0.01));
        })
        .response
        .on_hover_text("Sampled y extent, world units");
        ui.horizontal(|ui| {
            ui.label("z:");
            ui.add(egui::DragValue::new(&mut controls.domain_min.z).speed(0.01));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut controls.domain_max.z).speed(0.01));
        })
        .response
        .on_hover_text("Sampled z extent, world units");
    });

    // Hand the whole mirror to the seam (below); the rebuild that the dirty
    // flag asks for happens in app.rs right after this panel, so this frame's
    // snapshot and status-bar count already reflect the edits.
    apply(state, controls);
}

/// One collapsible section with a reset-to-defaults button in its header.
fn section(
    ui: &mut egui::Ui,
    section: Section,
    controls: &mut Controls,
    body: impl FnOnce(&mut egui::Ui, &mut Controls),
) {
    let id = ui.make_persistent_id(("controls_section", section.title()));
    let mut reset = false;
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        section.default_open(),
    )
    .show_header(ui, |ui| {
        ui.label(egui::RichText::new(section.title()).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            reset = ui
                .small_button("Reset")
                .on_hover_text(format!("Reset {} to defaults", section.title()))
                .clicked();
        });
    })
    .body(|ui| body(ui, controls));

    if reset {
        match section {
            Section::Display => controls.reset_display(),
            Section::Features => controls.reset_features(),
            Section::Coloring => controls.reset_coloring(),
            Section::Domain => controls.reset_domain(),
        }
    }
}

/// The whole edit→seam handoff, kept pure (no egui types) so `show` stays
/// thin and the wiring is testable without a window (the spec's testing rule,
/// and the ledger ruling that the sidebar must call seam setters, never
/// mutate scene state). Every seam setter owns its own clamping / swap rule
/// and marks the state dirty only if the value actually moved; afterwards the
/// mirror is refreshed from the effective state, so next frame's widgets
/// start from truth (clamped density, swapped extrema, …).
pub fn apply(state: &mut State, controls: &mut Controls) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_are_ordered_display_features_coloring_domain() {
        let titles: Vec<&str> = SECTIONS.iter().map(|section| section.title()).collect();
        assert_eq!(titles, ["Display", "Features", "Coloring", "Domain"]);
    }

    #[test]
    fn display_and_features_open_first_the_others_stay_collapsed() {
        assert!(Section::Display.default_open(), "Display is the primary knob group");
        assert!(Section::Features.default_open(), "Features is the second everyday group");
        assert!(!Section::Coloring.default_open(), "Coloring starts collapsed");
        assert!(!Section::Domain.default_open(), "Domain starts collapsed");
    }

    #[test]
    fn each_reset_restores_only_its_own_section() {
        let defaults = Controls::new();
        let mut controls = Controls::new();
        controls.density = 77;
        controls.arrow_scale = 3.5;
        controls.normalize = true;
        controls.colormap = Colormap::Turbo;
        controls.auto_fit = false;
        controls.color_min = -4.0;
        controls.color_max = 9.0;
        controls.streamlines = true;
        controls.particles = true;
        controls.speed = 2.7;
        controls.domain_min = Vec3::splat(-5.0);
        controls.domain_max = Vec3::splat(5.0);

        controls.reset_display();
        assert_eq!(controls.density, defaults.density);
        assert_eq!(controls.arrow_scale, defaults.arrow_scale);
        assert!(!controls.normalize);
        assert_eq!(controls.colormap, Colormap::Turbo, "Display reset leaves Coloring alone");
        assert_eq!(controls.speed, 2.7, "Display reset leaves Features alone");
        assert_eq!(controls.domain_max, Vec3::splat(5.0), "Display reset leaves Domain alone");

        controls.reset_features();
        assert!(!controls.streamlines);
        assert!(!controls.particles);
        assert_eq!(controls.speed, defaults.speed);
        assert_eq!(controls.colormap, Colormap::Turbo, "Features reset leaves Coloring alone");

        controls.reset_coloring();
        assert_eq!(controls.colormap, defaults.colormap);
        assert_eq!(controls.auto_fit, defaults.auto_fit);
        assert_eq!(controls.color_min, defaults.color_min);
        assert_eq!(controls.color_max, defaults.color_max);
        assert_eq!(controls.domain_min, Vec3::splat(-5.0), "Coloring reset leaves Domain alone");

        controls.reset_domain();
        assert_eq!(controls.domain_min, defaults.domain_min);
        assert_eq!(controls.domain_max, defaults.domain_max);
    }
}
