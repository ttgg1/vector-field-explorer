//! Bounded scalar CAS overlays with explicit dependency semantics (issue #233).
//!
//! A [`ScalarOverlay`] records one CAS scalar result plotted into the scene.
//! Its dependency says how the result resolves:
//!
//! - [`OverlayDependency::Linked`] resolves against the live committed
//!   [`ParameterSet`], plane frame and simulated time, so editing a parameter
//!   updates it.
//! - [`OverlayDependency::Frozen`] captures the expression, parameters, frame
//!   and time at creation, so later edits leave it unchanged.
//!
//! This is a pure binding/collection/validation model. It reuses the shipped
//! [`scalar_view_expr`] parser and [`PlaneFrame`]; it adds no second grammar,
//! solver, quadrature or physical model, and it never samples here. Invalid
//! domain coverage is the shipped frame/slice check, not a second
//! expression-level solver.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::parameters::ParameterSet;
use crate::plane::PlaneFrame;
use crate::scalar_analysis::validate_plane_frame;
use crate::scalar_view::{scalar_view_expr, slice_label};

/// Most overlays one scene may hold.
pub const MAX_OVERLAYS: usize = 4;
/// Longest overlay label in UTF-8 bytes.
pub const MAX_OVERLAY_LABEL_BYTES: usize = 64;

/// What the overlay draws, matching the shipped CAS scene targets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayLayer {
    /// Analytic scalar contours on the active slice.
    #[default]
    Contours,
    /// The potential gradient `E = -∇V` on the active slice.
    PotentialField,
}

impl OverlayLayer {
    /// The honest user-facing layer label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Contours => "contours",
            Self::PotentialField => "E = -∇V",
        }
    }
}

/// How an overlay resolves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayDependency {
    /// Follow the live committed parameters, frame and time.
    #[default]
    Linked,
    /// Keep the captured parameters, frame and time.
    Frozen,
}

impl OverlayDependency {
    /// The visible dependency label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Linked => "Linked",
            Self::Frozen => "Frozen",
        }
    }
}

/// A stable per-overlay identifier.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct OverlayId(pub u32);

impl std::fmt::Display for OverlayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// The values a frozen overlay captures at creation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayCapture {
    /// The committed parameters when the copy was captured.
    pub parameters: ParameterSet,
    /// The active plane frame when the copy was captured.
    #[serde(with = "crate::scalar_analysis::frame_serde")]
    pub frame: PlaneFrame,
    /// The simulated time when the copy was captured.
    pub time: f64,
}

/// One bounded CAS overlay record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalarOverlay {
    /// Stable identity within the owning set.
    #[serde(default)]
    pub id: OverlayId,
    /// User-visible label.
    #[serde(default)]
    pub label: String,
    /// The CAS scalar expression text.
    #[serde(default)]
    pub formula: String,
    /// What layer the result feeds.
    #[serde(default)]
    pub layer: OverlayLayer,
    /// Linked or frozen dependency semantics.
    #[serde(default)]
    pub dependency: OverlayDependency,
    /// Captured values; present exactly for a frozen overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<OverlayCapture>,
}

impl ScalarOverlay {
    /// Build a linked overlay record without validating it.
    pub fn linked(
        id: OverlayId,
        formula: impl Into<String>,
        layer: OverlayLayer,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            formula: formula.into(),
            layer,
            dependency: OverlayDependency::Linked,
            capture: None,
        }
    }

    /// Build a frozen overlay record without validating it.
    pub fn frozen(
        formula: impl Into<String>,
        layer: OverlayLayer,
        label: impl Into<String>,
        parameters: ParameterSet,
        frame: PlaneFrame,
        time: f64,
    ) -> Self {
        Self {
            id: OverlayId::default(),
            label: label.into(),
            formula: formula.into(),
            layer,
            dependency: OverlayDependency::Frozen,
            capture: Some(OverlayCapture {
                parameters,
                frame,
                time,
            }),
        }
    }

    /// Reject an empty/over-long label, an empty expression, a non-finite
    /// capture, or a dependency/capture mismatch.
    pub fn validate(&self) -> Result<(), String> {
        if self.label.is_empty() {
            return Err("overlay label must not be empty".to_owned());
        }
        if self.label.len() > MAX_OVERLAY_LABEL_BYTES {
            return Err(format!(
                "overlay label is {} bytes; at most {MAX_OVERLAY_LABEL_BYTES} are allowed",
                self.label.len()
            ));
        }
        if self.formula.trim().is_empty() {
            return Err("overlay expression must not be empty".to_owned());
        }
        match (self.dependency, &self.capture) {
            (OverlayDependency::Linked, None) => {}
            (OverlayDependency::Linked, Some(_)) => {
                return Err("a linked overlay must not carry captured values".to_owned());
            }
            (OverlayDependency::Frozen, None) => {
                return Err("a frozen overlay needs captured values".to_owned());
            }
            (OverlayDependency::Frozen, Some(capture)) => {
                capture.parameters.validate()?;
                validate_plane_frame(&capture.frame)?;
                if !capture.time.is_finite() {
                    return Err("frozen overlay captured time must be finite".to_owned());
                }
            }
        }
        Ok(())
    }

    /// Resolve this overlay against the live document values.
    ///
    /// A linked overlay uses `live`; a frozen overlay uses its capture. The
    /// expression is re-parsed with the resolved parameter set through the
    /// shipped [`scalar_view_expr`] seam, so a removed parameter or unsupported
    /// slice reference is an identifiable [`OverlayUnavailable`] reason.
    pub fn resolve(&self, live: OverlayLive<'_>) -> Result<OverlayResolution, OverlayUnavailable> {
        let (parameters, frame, time) = match (self.dependency, &self.capture) {
            (OverlayDependency::Linked, _) => (live.parameters.clone(), live.frame, live.time),
            (OverlayDependency::Frozen, Some(capture)) => {
                (capture.parameters.clone(), capture.frame, capture.time)
            }
            (OverlayDependency::Frozen, None) => {
                return Err(OverlayUnavailable {
                    reason: "frozen overlay has no captured values".to_owned(),
                });
            }
        };
        if !time.is_finite() {
            return Err(OverlayUnavailable {
                reason: "overlay time must be finite".to_owned(),
            });
        }
        validate_plane_frame(&frame).map_err(|reason| OverlayUnavailable {
            reason: format!("invalid overlay frame: {reason}"),
        })?;
        scalar_view_expr(&self.formula, &parameters).map_err(|error| OverlayUnavailable {
            reason: error.to_string(),
        })?;
        Ok(OverlayResolution {
            formula: self.formula.clone(),
            parameters,
            frame,
            time,
            dependency: self.dependency,
        })
    }

    /// The captured values a frozen overlay must show; `None` for linked.
    pub fn captured_summary(&self) -> Option<String> {
        let capture = self.capture.as_ref()?;
        Some(format!(
            "Captured values: {}; {} at t = {:.3}.",
            bindings_summary(&capture.parameters),
            slice_label(&capture.frame),
            capture.time
        ))
    }
}

/// The live document values a linked overlay resolves against.
#[derive(Debug, Clone, Copy)]
pub struct OverlayLive<'a> {
    /// The live committed parameters.
    pub parameters: &'a ParameterSet,
    /// The live scalar-view plane frame.
    pub frame: PlaneFrame,
    /// The live simulated time.
    pub time: f64,
}

/// A resolved overlay expression and the values it binds.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayResolution {
    /// The expression text.
    pub formula: String,
    /// The parameters the expression resolves against.
    pub parameters: ParameterSet,
    /// The frame the expression covers.
    pub frame: PlaneFrame,
    /// The simulated time for time-dependent expressions.
    pub time: f64,
    /// How the result resolved.
    pub dependency: OverlayDependency,
}

/// Why an overlay cannot currently produce values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayUnavailable {
    /// The identifiable user-facing reason.
    pub reason: String,
}

impl std::fmt::Display for OverlayUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

/// A bounded, uniquely-identified overlay collection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScalarOverlaySet {
    overlays: Vec<ScalarOverlay>,
}

impl ScalarOverlaySet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a set without validating or repairing it. This is a test-only
    /// constructor: production restore validates a `SceneDocument` overlay set
    /// instead of sanitizing arbitrary stored data here.
    pub fn from_overlays(overlays: Vec<ScalarOverlay>) -> Self {
        Self { overlays }
    }

    /// Number of overlays.
    pub fn len(&self) -> usize {
        self.overlays.len()
    }

    /// Whether the set holds no overlays.
    pub fn is_empty(&self) -> bool {
        self.overlays.is_empty()
    }

    /// The overlays in display order.
    pub fn as_slice(&self) -> &[ScalarOverlay] {
        &self.overlays
    }

    /// The overlays in display order.
    pub fn iter(&self) -> impl Iterator<Item = &ScalarOverlay> {
        self.overlays.iter()
    }

    /// One overlay by id.
    pub fn get(&self, id: OverlayId) -> Option<&ScalarOverlay> {
        self.overlays.iter().find(|overlay| overlay.id == id)
    }

    /// The next free stable id.
    pub fn next_id(&self) -> OverlayId {
        OverlayId(
            self.overlays
                .iter()
                .map(|overlay| overlay.id.0)
                .max()
                .map_or(1, |max| max.saturating_add(1)),
        )
    }

    /// Validate the cap, unique ids and every entry.
    pub fn validate(&self) -> Result<(), String> {
        if self.overlays.len() > MAX_OVERLAYS {
            return Err(format!(
                "overlay set has {} entries; at most {MAX_OVERLAYS} are allowed",
                self.overlays.len()
            ));
        }
        let mut seen = HashSet::with_capacity(self.overlays.len());
        for overlay in &self.overlays {
            overlay.validate()?;
            if !seen.insert(overlay.id) {
                return Err(format!("duplicate overlay id {}", overlay.id.0));
            }
        }
        Ok(())
    }

    /// Append a linked overlay while the cap has room.
    pub fn add_linked(
        &mut self,
        formula: impl Into<String>,
        layer: OverlayLayer,
        label: impl Into<String>,
    ) -> Result<OverlayId, String> {
        let id = self.next_id();
        let mut overlay = ScalarOverlay::linked(id, formula, layer, label);
        self.fill_default_label(&mut overlay);
        self.push(overlay)?;
        Ok(id)
    }

    /// Append a frozen copy capturing the supplied values while the cap has
    /// room.
    pub fn add_frozen(
        &mut self,
        formula: impl Into<String>,
        layer: OverlayLayer,
        label: impl Into<String>,
        parameters: ParameterSet,
        frame: PlaneFrame,
        time: f64,
    ) -> Result<OverlayId, String> {
        let id = self.next_id();
        let mut overlay = ScalarOverlay::frozen(formula, layer, label, parameters, frame, time);
        overlay.id = id;
        self.fill_default_label(&mut overlay);
        self.push(overlay)?;
        Ok(id)
    }

    /// Append a validated, uniquely-identified overlay while the cap has room.
    /// A rejected push never changes the set.
    pub fn push(&mut self, overlay: ScalarOverlay) -> Result<(), String> {
        if self.overlays.len() >= MAX_OVERLAYS {
            return Err(format!("at most {MAX_OVERLAYS} overlays are allowed"));
        }
        overlay.validate()?;
        if self
            .overlays
            .iter()
            .any(|existing| existing.id == overlay.id)
        {
            return Err(format!("duplicate overlay id {}", overlay.id.0));
        }
        self.overlays.push(overlay);
        Ok(())
    }

    /// Remove one overlay by id.
    pub fn remove(&mut self, id: OverlayId) -> Option<ScalarOverlay> {
        let index = self.overlays.iter().position(|overlay| overlay.id == id)?;
        Some(self.overlays.remove(index))
    }

    /// Resolve one overlay against the live document values.
    pub fn resolve(
        &self,
        id: OverlayId,
        live: OverlayLive<'_>,
    ) -> Result<OverlayResolution, OverlayUnavailable> {
        self.get(id)
            .ok_or_else(|| OverlayUnavailable {
                reason: format!("overlay {id} is no longer available"),
            })?
            .resolve(live)
    }

    /// Test-only repair helper: drop empty expressions, duplicate ids and
    /// frozen entries without a valid capture, truncate over-long labels on a
    /// character boundary, give empty labels a stable fallback, and enforce the
    /// cap while preserving order. Valid values are never overridden.
    /// Production import and restore do not call this; `SceneDocument::validate`
    /// rejects an invalid overlay set rather than repairing it.
    pub fn sanitized(mut self) -> Self {
        let mut seen = HashSet::with_capacity(self.overlays.len());
        let mut repaired = Vec::with_capacity(self.overlays.len().min(MAX_OVERLAYS));
        for mut overlay in self.overlays.drain(..) {
            if overlay.formula.trim().is_empty() || !seen.insert(overlay.id) {
                continue;
            }
            if overlay.dependency == OverlayDependency::Linked {
                overlay.capture = None;
            } else if !frozen_capture_is_valid(&overlay) {
                continue;
            }
            if overlay.label.is_empty() {
                overlay.label = format!("overlay {}", repaired.len() + 1);
            }
            if overlay.label.len() > MAX_OVERLAY_LABEL_BYTES {
                overlay.label = truncate_label(&overlay.label);
            }
            repaired.push(overlay);
            if repaired.len() >= MAX_OVERLAYS {
                break;
            }
        }
        Self { overlays: repaired }
    }

    fn fill_default_label(&self, overlay: &mut ScalarOverlay) {
        if overlay.label.trim().is_empty() {
            overlay.label = format!(
                "{} {}",
                overlay.dependency.label().to_ascii_lowercase(),
                overlay.id.0
            );
        }
    }
}

fn frozen_capture_is_valid(overlay: &ScalarOverlay) -> bool {
    let Some(capture) = &overlay.capture else {
        return false;
    };
    capture.parameters.validate().is_ok()
        && validate_plane_frame(&capture.frame).is_ok()
        && capture.time.is_finite()
}

fn truncate_label(label: &str) -> String {
    let mut end = MAX_OVERLAY_LABEL_BYTES.min(label.len());
    while end > 0 && !label.is_char_boundary(end) {
        end -= 1;
    }
    label[..end].to_owned()
}

/// The parameter bindings shown beside a CAS result and in a frozen summary.
pub fn bindings_summary(parameters: &ParameterSet) -> String {
    if parameters.entries.is_empty() {
        "no parameters".to_owned()
    } else {
        parameters
            .entries
            .iter()
            .map(|entry| format!("{} = {}", entry.name, entry.value))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The model assumptions shown beside the overlay choices.
pub const OVERLAY_ASSUMPTIONS: &str = "Assumes the analytic expression is valid at every sampled point on the captured slice; \
     values are sampled with the bound committed parameters, never re-derived. Missing parameters or a non-finite frame \
     make the overlay unavailable instead of an empty layer. Invalid-domain coverage is the shipped frame/slice check: \
     this layer adds no second expression-level domain solver, so an undefined point inside a valid slice is a \
     sampling/render concern.";

/// The honest limitation when a frozen copy is stored beside a drawn overlay.
pub const FROZEN_RENDER_NOTE: &str = "One gradient overlay can be drawn at a time. A frozen copy keeps its captured \
     expression, parameters, frame and time as a stored prediction; it is labelled here and is not drawn \
     simultaneously with the active linked gradient.";
