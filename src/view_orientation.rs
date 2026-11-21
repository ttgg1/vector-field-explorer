//! Display-only vertical world axis (issue #208).
//!
//! The user may choose which world axis points up on screen. Coordinates,
//! formulas, field values, probes and saved scenes stay in the existing x/y/z
//! frame; only the camera basis and therefore the rendered view change.
//!
//! [`UpAxis::Z`] is the default and applies a proper -90° rotation about X, so
//! world `+Z` is up on screen, matching the physics convention. The pre-#208
//! historical view corresponded to [`UpAxis::Y`] (world `+Y` on screen), which
//! remains available as a display option.
//!
//! The rotation is a world-to-display rotation: [`UpAxis::rotation`] maps the
//! chosen basis' up vector onto display `+Y`. [`UpAxis::basis`] returns the
//! camera's `(up, forward)` world vectors, which are the inverse rotation of
//! display `+Y` and the display azimuth reference. Every rotation is a proper
//! (positive-determinant) right-handed rotation, and its inverse reproduces
//! world positions for picking.

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Which world axis is used as the view's up direction.
///
/// The persisted tokens are lowercase (`z`, `x`, `y`). Display-only: never part
/// of a scene document, appearance profile or edit history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UpAxis {
    /// The physics-convention default. World `+Z` is up on screen.
    #[default]
    Z,
    /// World `+X` points up on screen.
    X,
    /// World `+Y` points up on screen; this was the historical pre-#208 view.
    Y,
}

impl UpAxis {
    /// Every selectable axis, in menu order.
    pub const ALL: [Self; 3] = [Self::Z, Self::X, Self::Y];

    /// The user-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Z => "Z up (default)",
            Self::X => "X up",
            Self::Y => "Y up",
        }
    }

    /// The camera's `(up, forward)` world vectors.
    ///
    /// `up` is the world direction that the rotation maps to display `+Y`;
    /// `forward` is the world direction at azimuth zero. Both are unit length
    /// and perpendicular, and `forward × up` is the camera's screen-right
    /// direction, so the basis is always right-handed.
    pub fn basis(self) -> (Vec3, Vec3) {
        let inverse = self.rotation().inverse();
        (inverse * Vec3::Y, inverse * Vec3::X)
    }

    /// The world-to-display rotation, applied last when drawing.
    ///
    /// `Z` is a quarter turn about X, mapping world `+Z` to display `+Y` and
    /// world `+Y` to display forward `-Z`. `X` is a half turn about `(1, 1, 0)`,
    /// mapping `+X` to display `+Y` and `+Z` to `-Z`. `Y` is the identity that
    /// preserves the old historical orientation. All three are proper rotations.
    pub fn rotation(self) -> Quat {
        match self {
            Self::Z => Quat::from_axis_angle(Vec3::X, -std::f32::consts::FRAC_PI_2),
            Self::X => {
                Quat::from_axis_angle(Vec3::new(1.0, 1.0, 0.0).normalize(), std::f32::consts::PI)
            }
            Self::Y => Quat::IDENTITY,
        }
    }
}

/// The stored axis name, decoded without rejecting an unknown value.
struct UpAxisName(String);

impl<'de> Deserialize<'de> for UpAxisName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Visitor;

        struct NameVisitor;

        impl Visitor<'_> for NameVisitor {
            type Value = UpAxisName;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an up-axis name")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(UpAxisName(value.to_owned()))
            }
        }

        deserializer.deserialize_identifier(NameVisitor)
    }
}

/// Decode the unit variant, repairing an unknown token to [`UpAxis::Z`] with
/// exactly one warning.
struct UpAxisVisitor;

impl<'de> serde::de::Visitor<'de> for UpAxisVisitor {
    type Value = UpAxis;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("z, x or y")
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::EnumAccess<'de>,
    {
        use serde::de::VariantAccess;

        let (name, variant) = data.variant::<UpAxisName>()?;
        let axis = match name.0.as_str() {
            "z" | "Z" => UpAxis::Z,
            "x" | "X" => UpAxis::X,
            "y" | "Y" => UpAxis::Y,
            other => {
                log::warn!("persisted up axis {other:?} is unknown; using the Z default");
                UpAxis::Z
            }
        };
        variant.unit_variant()?;
        Ok(axis)
    }
}

impl<'de> Deserialize<'de> for UpAxis {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_enum("UpAxis", &["z", "x", "y"], UpAxisVisitor)
    }
}
