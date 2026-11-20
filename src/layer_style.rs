//! Bounded per-layer and per-family appearance overrides (issue #84 slice A1).
//!
//! This is the pure model behind the Appearance panel: each drawn layer, and
//! each physics family inside the family-aware layers, may multiply opacity and
//! width or replace a colour that is already fixed. Global controls remain the
//! base, and resolution runs per attribute global -> family -> layer.
//!
//! Defaults are neutral (`1.0`, `1.0`, no colour), so an absent record renders
//! exactly the scene from before this slice. Nothing here touches the renderer,
//! the UI or scene documents; [`crate::persistence::AppSettings`] only stores
//! the record.

use serde::{Deserialize, Serialize};

use crate::color_ramp::Rgb;

/// Smallest accepted width multiplier.
pub const MIN_WIDTH_MULTIPLIER: f32 = 0.05;
/// Largest accepted width multiplier.
pub const MAX_WIDTH_MULTIPLIER: f32 = 4.0;

/// One drawn layer that owns appearance overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerId {
    Arrows,
    Flow,
    Sources,
    Induction,
    TeachingScene,
    Reference,
}

impl LayerId {
    /// Every layer, in [`LayerStyles::layers`] order.
    pub const ALL: [LayerId; 6] = [
        LayerId::Arrows,
        LayerId::Flow,
        LayerId::Sources,
        LayerId::Induction,
        LayerId::TeachingScene,
        LayerId::Reference,
    ];

    /// The label the Appearance panel shows.
    pub fn label(&self) -> &'static str {
        match self {
            LayerId::Arrows => "Arrows",
            LayerId::Flow => "Flow",
            LayerId::Sources => "Sources",
            LayerId::Induction => "Induction",
            LayerId::TeachingScene => "Teaching scenes",
            LayerId::Reference => "Reference",
        }
    }

    fn index(self) -> usize {
        match self {
            LayerId::Arrows => 0,
            LayerId::Flow => 1,
            LayerId::Sources => 2,
            LayerId::Induction => 3,
            LayerId::TeachingScene => 4,
            LayerId::Reference => 5,
        }
    }
}

/// The family axis inside Sources, Induction and Teaching scenes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsFamily {
    Electric,
    Magnetic,
}

impl PhysicsFamily {
    /// Every family, in [`LayerStyles::families`] order.
    pub const ALL: [PhysicsFamily; 2] = [PhysicsFamily::Electric, PhysicsFamily::Magnetic];

    /// The label the Appearance panel shows.
    pub fn label(&self) -> &'static str {
        match self {
            PhysicsFamily::Electric => "Electric",
            PhysicsFamily::Magnetic => "Magnetic",
        }
    }

    fn index(self) -> usize {
        match self {
            PhysicsFamily::Electric => 0,
            PhysicsFamily::Magnetic => 1,
        }
    }
}

/// Opacity, width and fixed-colour overrides for one layer or family.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerOverride {
    /// Opacity multiplier in `0.0..=1.0`.
    pub opacity: Option<f32>,
    /// Width multiplier in [`MIN_WIDTH_MULTIPLIER`]..=[`MAX_WIDTH_MULTIPLIER`].
    pub width: Option<f32>,
    /// Replacement colour, honoured only where the drawable owns a fixed
    /// colour; mapped colours are never replaced.
    pub color: Option<Rgb>,
}

impl LayerOverride {
    /// Reject non-finite or out-of-range values instead of clamping them.
    /// `field` is the record path, e.g. `layers[2]`.
    pub fn validate(&self, field: &str) -> Result<(), String> {
        if let Some(opacity) = self.opacity
            && (!opacity.is_finite() || !(0.0..=1.0).contains(&opacity))
        {
            return Err(format!(
                "{field}.opacity must be finite and in 0..=1, got {opacity}"
            ));
        }
        if let Some(width) = self.width
            && (!width.is_finite()
                || !(MIN_WIDTH_MULTIPLIER..=MAX_WIDTH_MULTIPLIER).contains(&width))
        {
            return Err(format!(
                "{field}.width must be finite and in \
                 {MIN_WIDTH_MULTIPLIER}..={MAX_WIDTH_MULTIPLIER}, got {width}"
            ));
        }
        Ok(())
    }

    /// Repair a record read from storage: a non-finite value becomes unset and
    /// a finite out-of-range value clamps into its bounds. Colours need no
    /// repair. Never panics.
    pub fn sanitized(&self) -> Self {
        Self {
            opacity: self
                .opacity
                .and_then(|opacity| opacity.is_finite().then(|| opacity.clamp(0.0, 1.0))),
            width: self.width.and_then(|width| {
                width
                    .is_finite()
                    .then(|| width.clamp(MIN_WIDTH_MULTIPLIER, MAX_WIDTH_MULTIPLIER))
            }),
            color: self.color,
        }
    }

    /// Whether no attribute is overridden.
    pub fn is_neutral(&self) -> bool {
        self.opacity.is_none() && self.width.is_none() && self.color.is_none()
    }
}

/// The persisted layer and family override records.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerStyles {
    /// Per-layer overrides, in [`LayerId::ALL`] order.
    pub layers: [LayerOverride; 6],
    /// Per-family overrides, in [`PhysicsFamily::ALL`] order.
    pub families: [LayerOverride; 2],
}

impl LayerStyles {
    /// Reject the first invalid override, naming its exact path.
    pub fn validate(&self) -> Result<(), String> {
        for (index, layer) in self.layers.iter().enumerate() {
            layer.validate(&format!("layers[{index}]"))?;
        }
        for (index, family) in self.families.iter().enumerate() {
            family.validate(&format!("families[{index}]"))?;
        }
        Ok(())
    }

    /// Repair every stored override; a record that validates is unchanged.
    pub fn sanitized(&self) -> Self {
        Self {
            layers: self.layers.map(|layer| layer.sanitized()),
            families: self.families.map(|family| family.sanitized()),
        }
    }

    /// Whether every layer and family record is neutral.
    pub fn is_neutral(&self) -> bool {
        self.layers.iter().all(LayerOverride::is_neutral)
            && self.families.iter().all(LayerOverride::is_neutral)
    }

    /// Resolve one layer, applying the optional family override before the
    /// layer override, per attribute. Defaults are `1.0`, `1.0`, no colour.
    pub fn resolve(&self, layer: LayerId, family: Option<PhysicsFamily>) -> ResolvedLayerStyle {
        let mut resolved = ResolvedLayerStyle {
            opacity: 1.0,
            width: 1.0,
            color: None,
        };
        if let Some(family) = family {
            resolved.apply(self.families[family.index()]);
        }
        resolved.apply(self.layers[layer.index()]);
        resolved
    }
}

/// Effective values after global, family and layer precedence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedLayerStyle {
    pub opacity: f32,
    pub width: f32,
    pub color: Option<Rgb>,
}

impl ResolvedLayerStyle {
    fn apply(&mut self, overrides: LayerOverride) {
        if let Some(opacity) = overrides.opacity {
            self.opacity = opacity;
        }
        if let Some(width) = overrides.width {
            self.width = width;
        }
        if let Some(color) = overrides.color {
            self.color = Some(color);
        }
    }
}
