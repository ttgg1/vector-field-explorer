//! Arrow style families and their geometry parameters (ticket 45).
//!
//! Two independent sets: [`ArrowStyle`] selects the 3D family and
//! [`ArrowStyle2D`] selects the 2D family (ticket 61). Changing one never
//! changes the other. Each set is deliberately small and extensible: one enum
//! variant, one geometry/draw-plan arm, and (for 3D) one branch in
//! `shaders.arrow.wgsl`.
//!
//! These types are persisted in the view record, so every numeric field has a
//! documented range and [`ArrowParams::sanitized`] is the single clamp used by
//! the seam, the sidebar and restore.

use serde::{Deserialize, Serialize};

/// Bounds of the arrow head length, as a fraction of the total arrow length.
pub const MIN_HEAD_LENGTH: f32 = 0.1;
pub const MAX_HEAD_LENGTH: f32 = 0.5;
/// Bounds of the arrow head width (radius), as a fraction of the length.
pub const MIN_HEAD_WIDTH: f32 = 0.02;
pub const MAX_HEAD_WIDTH: f32 = 0.3;
/// Bounds of the shaft radius, as a fraction of the length.
pub const MIN_SHAFT_RADIUS: f32 = 0.01;
pub const MAX_SHAFT_RADIUS: f32 = 0.12;
/// Bounds of the radial side count shared by Cone and Cylinder.
pub const MIN_SIDES: u32 = 4;
pub const MAX_SIDES: u32 = 16;

/// The arrow shape family drawn in the 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrowStyle {
    /// Tapered n-sided shaft with a matching cone head.
    #[default]
    Cone,
    /// The 42-vertex four-sided faceted head from ticket 44.
    Pyramid,
    /// Constant-radius n-sided shaft with a cone head.
    Cylinder,
    /// Flat camera-facing 3D ribbon family.
    Line,
}

/// The arrow family drawn in the 2D view (ticket 61).
///
/// Deliberately independent of [`ArrowStyle`]: the 2D selection never feeds the
/// 3D draw plan and vice versa. Adding a 2D family is one variant here plus one
/// arm in [`crate::renderer::SceneGPU::arrow_draw_plan`]; the sidebar selector
/// and persistence pick it up from [`ArrowStyle2D::ALL`] automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrowStyle2D {
    /// The flat nine-vertex silhouette drawn by `shaders.arrow2d.wgsl`.
    #[default]
    #[serde(rename = "Line")]
    Line,
}

impl ArrowStyle2D {
    /// Every 2D family, in the order the `2D arrow style` selector shows them.
    pub const ALL: [ArrowStyle2D; 1] = [ArrowStyle2D::Line];

    /// Short label for the 2D style selector.
    pub fn label(self) -> &'static str {
        match self {
            ArrowStyle2D::Line => "Line",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            ArrowStyle2D::Line => "Flat ribbon silhouette for the 2D view",
        }
    }

    /// Whether the sides slider changes this family's geometry.
    pub fn uses_sides(self) -> bool {
        match self {
            ArrowStyle2D::Line => false,
        }
    }
}

impl ArrowStyle {
    /// Every family, in the order the Display selector shows them.
    pub const ALL: [ArrowStyle; 4] = [
        ArrowStyle::Cone,
        ArrowStyle::Pyramid,
        ArrowStyle::Cylinder,
        ArrowStyle::Line,
    ];

    /// Short label for the style selector.
    pub fn label(self) -> &'static str {
        match self {
            ArrowStyle::Cone => "Cone",
            ArrowStyle::Pyramid => "Pyramid",
            ArrowStyle::Cylinder => "Cylinder",
            ArrowStyle::Line => "Line",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            ArrowStyle::Cone => "Tapered shaft with a round cone head",
            ArrowStyle::Pyramid => "Four-sided faceted shaft and head",
            ArrowStyle::Cylinder => "Constant-radius shaft with a round cone head",
            ArrowStyle::Line => "Flat ribbon that always faces the camera",
        }
    }

    /// The shader's style id: the value carried in `Globals.arrow_style.x`.
    pub fn id(self) -> u32 {
        match self {
            ArrowStyle::Cone => 0,
            ArrowStyle::Pyramid => 1,
            ArrowStyle::Cylinder => 2,
            ArrowStyle::Line => 3,
        }
    }

    /// Whether the sides slider changes this family's geometry.
    pub fn uses_sides(self) -> bool {
        matches!(self, ArrowStyle::Cone | ArrowStyle::Cylinder)
    }

    /// Vertices the 3D shader emits per arrow for this family.
    ///
    /// Cone and Cylinder: shaft `6 * sides`, head `3 * sides`, and a full
    /// `3 * (sides - 2)` fan under the head — `12 * sides - 6` in total.
    /// Pyramid: the ticket-44 42-vertex layout. Line: a 9-vertex ribbon.
    pub fn vertex_count(self, sides: u32) -> u32 {
        match self {
            ArrowStyle::Pyramid => 42,
            ArrowStyle::Cone | ArrowStyle::Cylinder => {
                12 * sides.clamp(MIN_SIDES, MAX_SIDES) - 6
            }
            ArrowStyle::Line => 9,
        }
    }
}

/// The small, organized set of arrow geometry knobs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArrowParams {
    /// Head length, as a fraction of the total arrow length.
    pub head_length: f32,
    /// Head width (radius), as a fraction of the total arrow length.
    pub head_width: f32,
    /// Shaft radius, as a fraction of the total arrow length.
    pub shaft_radius: f32,
    /// Radial side count, used by Cone and Cylinder.
    pub sides: u32,
}

impl Default for ArrowParams {
    fn default() -> Self {
        Self { head_length: 0.3, head_width: 0.12, shaft_radius: 0.055, sides: 12 }
    }
}

impl ArrowParams {
    /// Clamp every field into its documented range. A non-finite float has no
    /// usable direction, so it takes the default; finite values clamp.
    pub fn sanitized(self) -> Self {
        let defaults = Self::default();
        let finite_or = |value: f32, fallback: f32| {
            if value.is_finite() { value } else { fallback }
        };
        Self {
            head_length: finite_or(self.head_length, defaults.head_length)
                .clamp(MIN_HEAD_LENGTH, MAX_HEAD_LENGTH),
            head_width: finite_or(self.head_width, defaults.head_width)
                .clamp(MIN_HEAD_WIDTH, MAX_HEAD_WIDTH),
            shaft_radius: finite_or(self.shaft_radius, defaults.shaft_radius)
                .clamp(MIN_SHAFT_RADIUS, MAX_SHAFT_RADIUS),
            sides: self.sides.clamp(MIN_SIDES, MAX_SIDES),
        }
    }
}
