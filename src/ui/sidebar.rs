//! Left sidebar: every scene knob the widgets expose — sampling density,
//! arrow length modes and magnitude scale, colormap and color range, streamlines and
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
use crate::plane::{PlaneAxis, PlaneView, plane_coordinate};
use crate::viewmodel::{
    ArrowLengthMode,
    State,
    DEFAULT_DENSITY_2D,
    DEFAULT_MAGNITUDE_SCALE,
    MIN_DENSITY,
    MAX_DENSITY_2D,
    MAX_DENSITY_3D,
    MIN_ARROW_SCALE,
    MAX_ARROW_SCALE,
    MIN_MAGNITUDE_SCALE,
    MAX_MAGNITUDE_SCALE,
    MIN_SPEED,
    MAX_SPEED,
    MIN_DOMAIN_EXTENT,
    MAX_DOMAIN_EXTENT,
    clamp_domain_extent,
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

/// The effective domain box as the Domain header's summary line, e.g.
/// `x -1…1 · y -1…1 · z -1…1`. `include_z` is false when the view is 2D, so
/// the header never advertises an extent the user cannot see or sample
/// (ticket 50 passes the same flag to the row list). Numbers are rounded to at
/// most two decimals and integers lose their trailing `.00`, which keeps the
/// line short enough for the panel's 180 pt minimum.
pub fn domain_summary(domain: &Domain, include_z: bool) -> String {
    let axis = |name: &str, lo: f32, hi: f32| {
        format!("{name} {}…{}", extent_text(lo), extent_text(hi))
    };
    let mut parts = vec![
        axis("x", domain.min.x, domain.max.x),
        axis("y", domain.min.y, domain.max.y),
    ];
    if include_z {
        parts.push(axis("z", domain.min.z, domain.max.z));
    }
    parts.join(" · ")
}

/// One number in [`domain_summary`]: an integer when it rounds to one, else
/// at most two decimals with trailing zeros trimmed. Non-finite values cannot
/// reach the state, but the summary still renders something harmless if one
/// appears in a mirror mid-edit.
fn extent_text(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = if value.abs() < 0.005 {
        0.0
    } else {
        (value * 100.0).round() / 100.0
    };
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        let mut text = format!("{rounded:.2}");
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    }
}

/// Whether the Magnitude-scale control applies. Only `Magnitude` uses it, so
/// the widget is disabled everywhere else: no dead control that looks
/// adjustable but changes nothing. Pure so the rule is testable without a
/// window, matching the sidebar's other seams.
pub fn magnitude_scale_enabled(mode: ArrowLengthMode) -> bool {
    mode == ArrowLengthMode::Magnitude
}

/// Whether the shared arrow length scale applies. `Magnitude` lengths are the
/// field magnitude times the magnitude scale alone, so the arrow scale is
/// disabled there rather than silently ignored.
pub fn arrow_scale_enabled(mode: ArrowLengthMode) -> bool {
    mode != ArrowLengthMode::Magnitude
}

/// The knobs the widgets edit between frames. Starts at the same defaults as
/// a fresh 2D [`State`] so an untouched first sidebar frame never marks the
/// scene dirty (and the status bar counts the scene the user actually sees).
#[derive(Debug, Clone)]
pub struct Controls {
    pub density: u32,
    pub arrow_scale: f32,
    pub arrow_length_mode: ArrowLengthMode,
    pub magnitude_scale: f32,
    pub colormap: Colormap,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    pub streamlines: bool,
    pub particles: bool,
    pub speed: f32,
    pub domain_min: Vec3,
    pub domain_max: Vec3,
    /// Plane-by-plane mirror (ticket 23). The index is only handed to the
    /// seam when the slider is released, so a drag never resamples the field.
    pub plane: PlaneView,
    /// True while the plane slider is being dragged: the mirror must keep the
    /// in-flight value instead of following the state.
    pub plane_drag: bool,
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
            arrow_length_mode: ArrowLengthMode::Proportional,
            magnitude_scale: DEFAULT_MAGNITUDE_SCALE,
            colormap: Colormap::Viridis,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            streamlines: false,
            particles: false,
            speed: 1.0,
            domain_min: domain.min,
            domain_max: domain.max,
            plane: PlaneView::default(),
            plane_drag: false,
        }
    }

    /// Restore the Display section's defaults (density, arrow length mode and
    /// both scales, plane-by-plane).
    pub fn reset_display(&mut self) {
        let defaults = Controls::new();
        self.density = defaults.density;
        self.arrow_scale = defaults.arrow_scale;
        self.arrow_length_mode = defaults.arrow_length_mode;
        self.magnitude_scale = defaults.magnitude_scale;
        self.plane = defaults.plane;
        self.plane_drag = false;
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
    let grid = state.grid();
    let domain = state.domain;

    // The plane mirror follows the seam (which clamps/recentres it on grid
    // changes) except while its slider is mid-drag: the in-flight value has to
    // survive the frame so the slider does not snap back. The seam is only
    // handed the new index when the drag ends, so dragging never resamples.
    if !controls.plane_drag {
        controls.plane = state.plane;
    }
    let mut commit_plane = false;

    section(ui, Section::Display, controls, None, |ui, controls| {
        ui.horizontal(|ui| {
            ui.label(if sampling_3d { "Density (n³)" } else { "Density (n²)" });
            ui.add(egui::DragValue::new(&mut controls.density).range(MIN_DENSITY..=max_density))
                .on_hover_text("Samples per axis; 3D sampling makes it the cube of this count");
        });
        let arrow_scale_on = arrow_scale_enabled(controls.arrow_length_mode);
        let arrow_scale_text = if arrow_scale_on {
            "Arrow length (grid cells)"
        } else {
            "Arrow length (Proportional/Uniform)"
        };
        let arrow_scale_hover = if arrow_scale_on {
            "Arrow length as a multiple of one grid cell"
        } else {
            "Disabled in Magnitude mode; use Magnitude scale instead"
        };
        ui.add_enabled_ui(arrow_scale_on, |ui| {
            ui.add(
                egui::Slider::new(&mut controls.arrow_scale, MIN_ARROW_SCALE..=MAX_ARROW_SCALE)
                    .text(arrow_scale_text),
            );
            ui.horizontal(|ui| {
                ui.label("Exact length");
                ui.add(
                    egui::DragValue::new(&mut controls.arrow_scale)
                        .speed(0.01)
                        .range(MIN_ARROW_SCALE..=MAX_ARROW_SCALE)
                        .max_decimals(2),
                );
            });
        })
        .response
        .on_hover_text(arrow_scale_hover)
        .on_disabled_hover_text("Disabled in Magnitude mode; use Magnitude scale instead");
        ui.horizontal(|ui| {
            ui.label("Length mode");
            for mode in ArrowLengthMode::ALL {
                ui.selectable_value(&mut controls.arrow_length_mode, mode, mode.label())
                    .on_hover_text(mode.tooltip());
            }
        });
        ui.add_enabled_ui(magnitude_scale_enabled(controls.arrow_length_mode), |ui| {
            ui.horizontal(|ui| {
                ui.label("Magnitude scale");
                ui.add(
                    egui::DragValue::new(&mut controls.magnitude_scale)
                        .speed(0.01)
                        .range(MIN_MAGNITUDE_SCALE..=MAX_MAGNITUDE_SCALE)
                        .max_decimals(2),
                );
            });
        })
        .response
        .on_hover_text("World length per unit of field magnitude");
        if sampling_3d {
            commit_plane |= plane_controls(ui, controls, &domain, grid);
        }
    });

    section(ui, Section::Features, controls, None, |ui, controls| {
        ui.checkbox(&mut controls.streamlines, "Streamlines")
            .on_hover_text("Trace field lines from a grid of seeds");
        ui.checkbox(&mut controls.particles, "Particles")
            .on_hover_text("Animate dots carried by the field (3D view)");
        ui.add(
            egui::Slider::new(&mut controls.speed, MIN_SPEED..=MAX_SPEED).text("Trace speed"),
        )
        .on_hover_text("How fast traces advance, in grid cells per second");
    });

    section(ui, Section::Coloring, controls, None, |ui, controls| {
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
    // per-axis swap normalizes them (mirrored back below so the widgets show
    // the effective box). The header summary shows the box while collapsed;
    // z is omitted in 2D (ticket 50 hides the row itself later).
    section(
        ui,
        Section::Domain,
        controls,
        Some(domain_summary(&state.domain, sampling_3d)),
        |ui, controls| {
            domain_axis_row(ui, "x", &mut controls.domain_min.x, &mut controls.domain_max.x);
            domain_axis_row(ui, "y", &mut controls.domain_min.y, &mut controls.domain_max.y);
            domain_axis_row(ui, "z", &mut controls.domain_min.z, &mut controls.domain_max.z);
            ui.add_space(ui.spacing().item_spacing.y);
            if ui
                .button("Reset domain")
                .on_hover_text("Restore x, y, z to -1…1")
                .clicked()
            {
                // Same seam as the generic header button, so there is one
                // reset path.
                controls.reset_domain();
            }
        },
    );

    // Hand the whole mirror to the seam (below); the rebuild that the dirty
    // flag asks for happens in app.rs right after this panel, so this frame's
    // snapshot and status-bar count already reflect the edits. The plane index
    // is separate: it commits on toggle/axis changes and on slider release
    // only (ticket 23), so a drag never re-samples the field.
    if commit_plane {
        state.set_plane(controls.plane);
        controls.plane = state.plane;
    }
    apply(state, controls);
}

/// Draw the plane-by-plane controls (3D sampling only) and report whether the
/// selection should be committed to the seam this frame. The slider commits on
/// `drag_stopped` (or a click/keyboard change), not on every dragged frame.
fn plane_controls(
    ui: &mut egui::Ui,
    controls: &mut Controls,
    domain: &Domain,
    grid: glam::UVec3,
) -> bool {
    let mut commit = ui
        .checkbox(&mut controls.plane.enabled, "Plane by plane")
        .on_hover_text("Highlight one sampled slice and fade the rest of the cube")
        .changed();
    if !controls.plane.enabled {
        return commit;
    }

    ui.horizontal(|ui| {
        ui.label("Axis");
        for axis in PlaneAxis::ALL {
            commit |= ui
                .selectable_value(&mut controls.plane.axis, axis, axis.label().to_uppercase())
                .on_hover_text("Axis the selected plane is perpendicular to")
                .changed();
        }
    });

    let count = controls.plane.axis.count(grid);
    // An axis switch may leave the old index past the new axis's last sample.
    controls.plane.index = controls.plane.index.min(count.saturating_sub(1));
    if count <= 1 {
        ui.label(format!(
            "{} = {:.2}  (1 / 1) — one slice",
            controls.plane.axis.label(),
            plane_coordinate(domain, grid, controls.plane.axis, controls.plane.index),
        ));
        return commit;
    }

    let response = ui.add(
        egui::Slider::new(&mut controls.plane.index, 0..=count - 1)
            .text("Plane")
            .integer(),
    );
    if response.drag_started() {
        controls.plane_drag = true;
    }
    if response.drag_stopped() {
        controls.plane_drag = false;
        commit = true;
    }
    // A click on the track or a keyboard step is already a completed change;
    // only a live drag waits for the release.
    if response.changed() && !response.dragged() {
        commit = true;
    }
    let index = controls.plane.index;
    ui.label(format!(
        "{} = {:.2}  ({} / {})",
        controls.plane.axis.label(),
        plane_coordinate(domain, grid, controls.plane.axis, index),
        index + 1,
        count,
    ));
    commit
}

/// One axis's min/max row: the axis label names the component, both drag
/// values are bounded by the seam's single extent range, and the tooltips name
/// the world-unit meaning. The row is the same in 2D and 3D for now; ticket 50
/// will decide which rows to render.
fn domain_axis_row(ui: &mut egui::Ui, axis: &str, min: &mut f32, max: &mut f32) {
    ui.horizontal(|ui| {
        ui.label(format!("{axis}:"));
        ui.add(
            egui::DragValue::new(min)
                .speed(0.01)
                .range(MIN_DOMAIN_EXTENT..=MAX_DOMAIN_EXTENT)
                .max_decimals(2),
        )
        .on_hover_text(format!("Minimum {axis} of the sampled box, world units"));
        ui.label("to");
        ui.add(
            egui::DragValue::new(max)
                .speed(0.01)
                .range(MIN_DOMAIN_EXTENT..=MAX_DOMAIN_EXTENT)
                .max_decimals(2),
        )
        .on_hover_text(format!("Maximum {axis} of the sampled box, world units"));
    })
    .response
    .on_hover_text(format!("Sampled {axis} extent, world units"));
}

/// One collapsible section with a reset-to-defaults button in its header.
/// `summary`, when present, is a second header line that stays visible while
/// the section is collapsed (the Domain box needs to be readable without
/// opening it).
fn section(
    ui: &mut egui::Ui,
    section: Section,
    controls: &mut Controls,
    summary: Option<String>,
    body: impl FnOnce(&mut egui::Ui, &mut Controls),
) {
    let id = ui.make_persistent_id(("controls_section", section.title()));
    let mut reset = false;
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        section.default_open(),
    )
    .show_header(ui, |ui| match summary {
        Some(summary) => {
            ui.vertical(|ui| {
                section_header_line(ui, section, &mut reset);
                ui.add(egui::Label::new(summary).wrap()).on_hover_text(
                    "Current sampled box, world units; drag the rows below to change it",
                );
            });
        }
        None => section_header_line(ui, section, &mut reset),
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

/// The title row every section header shares: the strong title and the
/// generic reset button in the right-hand corner. The Domain header wraps this
/// in a vertical layout to add its summary line.
fn section_header_line(ui: &mut egui::Ui, section: Section, reset: &mut bool) {
    ui.label(egui::RichText::new(section.title()).strong());
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Full-size button, not `small_button`: the section action gets a
        // normal click target. (In egui 0.37 `small_button` only trims
        // vertical padding, so this is about target size, not type size.)
        *reset = ui
            .button("Reset")
            .on_hover_text(format!("Reset {} to defaults", section.title()))
            .clicked();
    });
}

/// Keep a dragged extent honest before the seam sees it: NaN has no direction,
/// so the previous effective value stays (the box must not jump); every other
/// value passes through the one bounded clamp.
fn extent_or_previous(value: f32, previous: f32) -> f32 {
    if value.is_nan() {
        previous
    } else {
        clamp_domain_extent(value)
    }
}

/// The whole edit→seam handoff, kept pure (no egui types) so `show` stays
/// thin and the wiring is testable without a window (the spec's testing rule,
/// and the ledger ruling that the sidebar must call seam setters, never
/// mutate scene state). Every seam setter owns its own clamping / swap rule
/// and marks the state dirty only if the value actually moved; afterwards the
/// mirror is refreshed from the effective state, so next frame's widgets
/// start from truth (clamped density, swapped extrema, …).
///
/// Plane-by-plane is deliberately absent: its index commits on slider release
/// (ticket 23), not once per frame, so it has its own handoff in `show`.
pub fn apply(state: &mut State, controls: &mut Controls) {
    state.set_density(controls.density);
    state.set_arrow_scale(controls.arrow_scale);
    state.set_arrow_length_mode(controls.arrow_length_mode);
    state.set_magnitude_scale(controls.magnitude_scale);
    state.set_colormap(controls.colormap);
    state.set_auto_fit(controls.auto_fit);
    state.set_color_range(controls.color_min, controls.color_max);
    state.set_streamlines(controls.streamlines);
    state.set_particles(controls.particles);
    state.set_speed(controls.speed);
    // Domain extents are bounded here, before `set_domain`'s per-axis swap,
    // so the UI's `.range` and this handoff share one clamp.
    let domain_min = Vec3::new(
        extent_or_previous(controls.domain_min.x, state.domain.min.x),
        extent_or_previous(controls.domain_min.y, state.domain.min.y),
        extent_or_previous(controls.domain_min.z, state.domain.min.z),
    );
    let domain_max = Vec3::new(
        extent_or_previous(controls.domain_max.x, state.domain.max.x),
        extent_or_previous(controls.domain_max.y, state.domain.max.y),
        extent_or_previous(controls.domain_max.z, state.domain.max.z),
    );
    state.set_domain(domain_min, domain_max);

    controls.density = state.density;
    controls.arrow_scale = state.arrow_scale;
    controls.arrow_length_mode = state.arrow_length_mode;
    controls.magnitude_scale = state.magnitude_scale;
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
