//! Validated viewport and reference styling (issue #82, slice A).
//!
//! The record owns the colours and line widths the reference geometry is drawn
//! with, plus the viewport background. This slice stores and validates it and
//! applies it to reference geometry only: the renderer clear colour, capture,
//! export and UI controls are a later slice. The background therefore records
//! the colour the renderer would use today without changing that path.
//!
//! [`Default`] is the shipped dark-laboratory look and reproduces the existing
//! `reference.rs` constants exactly, so legacy callers stay byte-identical.

use serde::{Deserialize, Serialize};

use crate::reference::{
    AXIS_WIDTH as DEFAULT_AXIS_WIDTH, AXIS_X_COLOR as DEFAULT_AXIS_X_COLOR,
    AXIS_Y_COLOR as DEFAULT_AXIS_Y_COLOR, AXIS_Z_COLOR as DEFAULT_AXIS_Z_COLOR,
    EDGE_COLOR as DEFAULT_BOX_COLOR, EDGE_WIDTH as DEFAULT_BOX_WIDTH,
    GRID_COLOR as DEFAULT_GRID_COLOR, GRID_WIDTH as DEFAULT_GRID_WIDTH,
};

/// Smallest accepted reference line width, world units.
pub const MIN_LINE_WIDTH: f32 = 0.001;
/// Largest accepted reference line width, world units.
pub const MAX_LINE_WIDTH: f32 = 0.05;

/// Linear RGB of the dark viewport clear colour.
///
/// `AnimationSnapshot::capture` derives it today from
/// `egui::Rgba::from(egui::Visuals::dark().panel_fill).to_array()`; the dark
/// panel fill is `Color32::from_gray(27)`, whose linear value is
/// `((27/255 + 0.055) / 1.055)^2.4`. Recording it keeps the scene untouched
/// while giving slice B the exact current colour to wire in.
pub const DEFAULT_BACKGROUND: [f32; 3] = [0.010960094, 0.010960094, 0.010960094];

/// The colours and line widths used to draw reference geometry, plus the
/// viewport background.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewportStyle {
    /// Linear 0..=1 background colour. Not applied by this slice.
    pub background: [f32; 3],
    /// Subdivided face grid or flat 2D lattice.
    pub grid_color: [f32; 4],
    /// Domain box edges.
    pub box_color: [f32; 4],
    /// World X axis (red-ish).
    pub axis_x_color: [f32; 4],
    /// World Y axis (green-ish).
    pub axis_y_color: [f32; 4],
    /// World Z axis (blue-ish).
    pub axis_z_color: [f32; 4],
    pub grid_width: f32,
    pub box_width: f32,
    pub axis_width: f32,
}

impl Default for ViewportStyle {
    fn default() -> Self {
        Self {
            background: DEFAULT_BACKGROUND,
            grid_color: DEFAULT_GRID_COLOR,
            box_color: DEFAULT_BOX_COLOR,
            axis_x_color: DEFAULT_AXIS_X_COLOR,
            axis_y_color: DEFAULT_AXIS_Y_COLOR,
            axis_z_color: DEFAULT_AXIS_Z_COLOR,
            grid_width: DEFAULT_GRID_WIDTH,
            box_width: DEFAULT_BOX_WIDTH,
            axis_width: DEFAULT_AXIS_WIDTH,
        }
    }
}

impl ViewportStyle {
    /// Reject any non-finite component and any value outside its own bounds.
    /// Widths are bounded independently: no ordering between grid, box and
    /// axis widths is implied or enforced.
    pub fn validate(&self) -> Result<(), String> {
        validate_background("background", self.background)?;
        validate_color("grid_color", self.grid_color)?;
        validate_color("box_color", self.box_color)?;
        validate_color("axis_x_color", self.axis_x_color)?;
        validate_color("axis_y_color", self.axis_y_color)?;
        validate_color("axis_z_color", self.axis_z_color)?;
        validate_width("grid_width", self.grid_width)?;
        validate_width("box_width", self.box_width)?;
        validate_width("axis_width", self.axis_width)?;
        Ok(())
    }

    /// Repair a record read from storage: a non-finite field falls back to the
    /// default for that field, while a finite out-of-range field clamps into
    /// its bounds. Never panics.
    pub fn sanitized(self) -> Self {
        let default = Self::default();
        Self {
            background: sanitize_background(self.background, default.background),
            grid_color: sanitize_color(self.grid_color, default.grid_color),
            box_color: sanitize_color(self.box_color, default.box_color),
            axis_x_color: sanitize_color(self.axis_x_color, default.axis_x_color),
            axis_y_color: sanitize_color(self.axis_y_color, default.axis_y_color),
            axis_z_color: sanitize_color(self.axis_z_color, default.axis_z_color),
            grid_width: sanitize_width(self.grid_width, default.grid_width),
            box_width: sanitize_width(self.box_width, default.box_width),
            axis_width: sanitize_width(self.axis_width, default.axis_width),
        }
    }

    /// The shipped dark-laboratory look (identical to [`Default`]).
    pub fn dark_laboratory() -> Self {
        Self::default()
    }

    /// A light paper look for screenshots and print-style exports.
    pub fn white_paper() -> Self {
        let style = Self {
            // Linear RGB of `Visuals::light().panel_fill` (`from_gray(248)`).
            background: [0.9386857, 0.9386857, 0.9386857],
            grid_color: [0.72, 0.74, 0.78, 1.0],
            box_color: [0.42, 0.45, 0.52, 1.0],
            axis_x_color: [0.72, 0.10, 0.12, 1.0],
            axis_y_color: [0.10, 0.52, 0.16, 1.0],
            axis_z_color: [0.10, 0.24, 0.72, 1.0],
            grid_width: 0.005,
            box_width: 0.010,
            axis_width: 0.012,
        };
        debug_assert!(style.validate().is_ok());
        style
    }

    /// A bright, high-contrast look on a black background.
    pub fn high_contrast() -> Self {
        let style = Self {
            background: [0.0, 0.0, 0.0],
            grid_color: [0.55, 0.58, 0.62, 1.0],
            box_color: [0.95, 0.95, 0.95, 1.0],
            axis_x_color: [1.0, 0.15, 0.15, 1.0],
            axis_y_color: [0.20, 1.0, 0.25, 1.0],
            axis_z_color: [0.25, 0.45, 1.0, 1.0],
            grid_width: 0.008,
            box_width: 0.016,
            axis_width: 0.022,
        };
        debug_assert!(style.validate().is_ok());
        style
    }
}

fn validate_background(name: &str, color: [f32; 3]) -> Result<(), String> {
    for (index, component) in color.iter().enumerate() {
        if !in_color_range(*component) {
            return Err(format!(
                "{name}[{index}] must be finite and in 0..=1, got {component}"
            ));
        }
    }
    Ok(())
}

fn validate_color(name: &str, color: [f32; 4]) -> Result<(), String> {
    for (index, component) in color.iter().enumerate() {
        if !in_color_range(*component) {
            return Err(format!(
                "{name}[{index}] must be finite and in 0..=1, got {component}"
            ));
        }
    }
    Ok(())
}

fn validate_width(name: &str, width: f32) -> Result<(), String> {
    if !width.is_finite() || !(MIN_LINE_WIDTH..=MAX_LINE_WIDTH).contains(&width) {
        return Err(format!(
            "{name} must be finite and in {MIN_LINE_WIDTH}..={MAX_LINE_WIDTH}, got {width}"
        ));
    }
    Ok(())
}

fn in_color_range(component: f32) -> bool {
    component.is_finite() && (0.0..=1.0).contains(&component)
}

fn sanitize_background(color: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    if color.iter().all(|component| component.is_finite()) {
        color.map(|component| component.clamp(0.0, 1.0))
    } else {
        fallback
    }
}

fn sanitize_color(color: [f32; 4], fallback: [f32; 4]) -> [f32; 4] {
    if color.iter().all(|component| component.is_finite()) {
        color.map(|component| component.clamp(0.0, 1.0))
    } else {
        fallback
    }
}

fn sanitize_width(width: f32, fallback: f32) -> f32 {
    if width.is_finite() {
        width.clamp(MIN_LINE_WIDTH, MAX_LINE_WIDTH)
    } else {
        fallback
    }
}
