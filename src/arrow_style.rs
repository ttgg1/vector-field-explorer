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

/// The arrow shape family drawn in the 3D view (ticket 64).
///
/// The persisted names are stable: old records used `Pyramid` for the cone and
/// `Line` for the flat ribbon, so those spellings are aliases here. The new
/// headless family is `Streamline` on disk and *Line* in the UI; it cannot be
/// called `Line` on disk without making old records ambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrowStyle {
    /// Tapered n-sided shaft with a matching cone head.
    #[default]
    #[serde(alias = "Pyramid")]
    Cone,
    /// Constant-radius n-sided shaft with a far-end cap and no head.
    Cylinder,
    /// Flat camera-facing ribbon with a shaft and an arrowhead.
    #[serde(alias = "Line")]
    Flat,
    /// Thin headless camera-facing ribbon, like a streamline segment.
    Streamline,
}

/// The arrow family drawn in the 2D view (ticket 61, split in ticket 64).
///
/// Deliberately independent of [`ArrowStyle`]: the 2D selection never feeds the
/// 3D draw plan and vice versa. Adding a 2D family is one variant here plus one
/// arm in [`crate::renderer::SceneGPU::arrow_draw_plan`]; the sidebar selector
/// and persistence pick it up from [`ArrowStyle2D::ALL`] automatically.
///
/// As in the 3D set, old records' `Line` spelling meant the flat ribbon, so it
/// aliases [`ArrowStyle2D::Flat`] and the new headless family persists as
/// `Streamline` while showing the label *Line*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrowStyle2D {
    /// The flat nine-vertex silhouette drawn by `shaders.arrow2d.wgsl`.
    #[default]
    #[serde(alias = "Line")]
    Flat,
    /// A thin headless shaft quad drawn by `shaders.arrow2d.wgsl`.
    Streamline,
}

impl ArrowStyle2D {
    /// Every 2D family, in the order the `2D arrow style` selector shows them.
    pub const ALL: [ArrowStyle2D; 2] = [ArrowStyle2D::Flat, ArrowStyle2D::Streamline];

    /// Short label for the 2D style selector.
    pub fn label(self) -> &'static str {
        match self {
            ArrowStyle2D::Flat => "Flat",
            ArrowStyle2D::Streamline => "Line",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            ArrowStyle2D::Flat => "Flat ribbon with an arrowhead",
            ArrowStyle2D::Streamline => "Thin headless line, like a streamline",
        }
    }

    /// The value carried in `Globals.arrow_style.x` while the 2D view paints.
    pub fn id(self) -> u32 {
        match self {
            ArrowStyle2D::Flat => 0,
            ArrowStyle2D::Streamline => 1,
        }
    }

    /// Whether the sides slider changes this family's geometry.
    pub fn uses_sides(self) -> bool {
        false
    }

    /// Whether the head length/width sliders apply to this family.
    pub fn uses_head(self) -> bool {
        matches!(self, ArrowStyle2D::Flat)
    }

    /// Vertices the 2D shader emits per arrow for this family.
    pub fn vertex_count(self, _sides: u32) -> u32 {
        match self {
            ArrowStyle2D::Flat => 9,
            ArrowStyle2D::Streamline => 6,
        }
    }
}

impl ArrowStyle {
    /// Every family, in the order the Display selector shows them.
    pub const ALL: [ArrowStyle; 4] = [
        ArrowStyle::Cone,
        ArrowStyle::Cylinder,
        ArrowStyle::Flat,
        ArrowStyle::Streamline,
    ];

    /// Short label for the style selector.
    pub fn label(self) -> &'static str {
        match self {
            ArrowStyle::Cone => "Cone",
            ArrowStyle::Cylinder => "Cylinder",
            ArrowStyle::Flat => "Flat",
            ArrowStyle::Streamline => "Line",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            ArrowStyle::Cone => "Tapered shaft with a round cone head",
            ArrowStyle::Cylinder => "Plain cylinder, no arrowhead",
            ArrowStyle::Flat => "Flat ribbon with an arrowhead",
            ArrowStyle::Streamline => "Thin headless line, like a streamline",
        }
    }

    /// The shader's style id: the value carried in `Globals.arrow_style.x`.
    pub fn id(self) -> u32 {
        match self {
            ArrowStyle::Cone => 0,
            ArrowStyle::Cylinder => 1,
            ArrowStyle::Flat => 2,
            ArrowStyle::Streamline => 3,
        }
    }

    /// Whether the sides slider changes this family's geometry.
    pub fn uses_sides(self) -> bool {
        matches!(self, ArrowStyle::Cone | ArrowStyle::Cylinder)
    }

    /// Whether the head length/width sliders apply to this family.
    pub fn uses_head(self) -> bool {
        matches!(self, ArrowStyle::Cone | ArrowStyle::Flat)
    }

    /// Vertices the 3D shader emits per arrow for this family.
    ///
    /// Cone: `6 * sides` shaft + `3 * sides` head + `3 * (sides - 2)` plate —
    /// `12 * sides - 6` in total. Cylinder: the same `6 * sides` shaft plus only
    /// the `3 * (sides - 2)` far-end cap — `9 * sides - 6`. Flat: a nine-vertex
    /// ribbon. Line: a six-vertex headless ribbon quad.
    pub fn vertex_count(self, sides: u32) -> u32 {
        let sides = sides.clamp(MIN_SIDES, MAX_SIDES);
        match self {
            ArrowStyle::Cone => 12 * sides - 6,
            ArrowStyle::Cylinder => 9 * sides - 6,
            ArrowStyle::Flat => 9,
            ArrowStyle::Streamline => 6,
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
