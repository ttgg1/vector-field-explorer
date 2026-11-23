//! Issue #222: a bounded, persisted experiment notebook.
//!
//! A [`NotebookEntry`] pairs a learner's prediction/note with an immutable
//! [`MeasurementSnapshot`] and the [`SceneDocument`] that produced it. This
//! module is pure model code: it reuses the shipped Gauss-shell reading and the
//! shipped versioned scene serializer, and it never computes a second field,
//! flux, quadrature or scene format.
//!
//! Snapshot values are owned copies. A live scene, parameter or clock edit can
//! therefore never mutate a pinned entry; loading a captured scene only reads
//! it back.

use serde::{Deserialize, Serialize};

use crate::field_probe::IntegralStatus;
use crate::maxwell::MaxwellScene;
use crate::parameters::ParameterSet;
use crate::persistence::ViewRecord;
use crate::scene_document::{SceneCamera, SceneDocument};
use crate::sources::FieldFamily;
use crate::ui::gauss_shell::{GaussShellReading, GaussShellReadingKind};

/// The notebook payload version this module writes.
pub const NOTEBOOK_VERSION: u32 = 1;
/// The most pinned entries a notebook stores.
pub const MAX_NOTEBOOK_ENTRIES: usize = 32;
/// The method label stored for a shipped Gauss-shell measurement.
pub const SHELL_METHOD: &str =
    "Gauss-shell flux at r through the shipped full/half-resolution quadrature";
/// The normalized teaching unit label stored for a shell measurement.
pub const SHELL_UNITS: &str = "normalized teaching units, k = 1";

/// The field family stored in a snapshot. Kept independent of the live model's
/// enum so the persisted shape is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotebookFamily {
    #[default]
    Electric,
    Magnetic,
}

impl From<FieldFamily> for NotebookFamily {
    fn from(family: FieldFamily) -> Self {
        match family {
            FieldFamily::Electric => Self::Electric,
            FieldFamily::Magnetic => Self::Magnetic,
        }
    }
}

impl NotebookFamily {
    /// The stable display label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Electric => "Electric",
            Self::Magnetic => "Magnetic",
        }
    }
}

/// The geometry a snapshot was taken at. Only the shell form exists in v1; a
/// later measurement adds another variant without changing this shape's core.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotebookGeometry {
    /// A Gauss-shell reading at the charged shell radius `R` and probe radius
    /// `r`.
    Shell {
        shell_radius: f64,
        probe_radius: f64,
    },
}

impl Default for NotebookGeometry {
    fn default() -> Self {
        Self::Shell {
            shell_radius: 0.0,
            probe_radius: 0.0,
        }
    }
}

impl NotebookGeometry {
    /// The probe radius `r` the reading was taken at.
    pub fn probe_radius(self) -> f64 {
        match self {
            Self::Shell { probe_radius, .. } => probe_radius,
        }
    }

    /// The charged shell radius `R` in the captured scene.
    pub fn shell_radius(self) -> f64 {
        match self {
            Self::Shell { shell_radius, .. } => shell_radius,
        }
    }

    /// A compact human-readable geometry summary.
    pub fn text(self) -> String {
        match self {
            Self::Shell {
                shell_radius,
                probe_radius,
            } => format!("Shell R = {shell_radius:.3}, probe r = {probe_radius:.3}"),
        }
    }
}

/// The stored measured value: an estimate with its quadrature uncertainty, or
/// an explicit unavailability. An excluded sample is never stored as zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotebookValue {
    /// The shipped quadrature estimate and its combined uncertainty.
    Estimate { value: f64, uncertainty: f64 },
    /// The measurement could not be made; the reason is preserved verbatim.
    Unavailable { reason: String },
}

impl Default for NotebookValue {
    fn default() -> Self {
        Self::Unavailable {
            reason: String::new(),
        }
    }
}

impl NotebookValue {
    /// The estimate pair, or `None` when the reading is unavailable.
    pub fn estimate(&self) -> Option<(f64, f64)> {
        match self {
            Self::Estimate { value, uncertainty } => Some((*value, *uncertainty)),
            Self::Unavailable { .. } => None,
        }
    }

    /// A compact status label used by the list and comparison.
    pub fn text(&self) -> String {
        match self {
            Self::Estimate { value, uncertainty } => {
                format!("{value:.6} +/- {uncertainty:.2e} (quadrature)")
            }
            Self::Unavailable { reason } => format!("unavailable: {reason}"),
        }
    }

    fn is_valid(&self) -> bool {
        match self {
            Self::Estimate { value, uncertainty } => {
                value.is_finite() && uncertainty.is_finite() && *uncertainty >= 0.0
            }
            Self::Unavailable { reason } => !reason.trim().is_empty(),
        }
    }
}

/// One immutable measurement captured with the learner's context.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MeasurementSnapshot {
    /// Which field family the measurement belongs to.
    pub family: NotebookFamily,
    /// The typed geometry (v1: shell radii).
    pub geometry: NotebookGeometry,
    /// The simulated time the reading was taken at.
    pub time: f64,
    /// A cloned committed parameter set at capture time.
    pub parameters: ParameterSet,
    /// The method that produced the value, as shown to the learner.
    pub method: String,
    /// The measured value or its explicit unavailability.
    pub value: NotebookValue,
    /// The model's unit statement.
    pub units: String,
}

impl MeasurementSnapshot {
    /// Capture one shell measurement. All live inputs are cloned; nothing keeps
    /// a reference to the scene, parameters or reading.
    pub fn shell(
        scene: &MaxwellScene,
        probe_radius: f64,
        family: FieldFamily,
        time: f64,
        parameters: ParameterSet,
        reading: &GaussShellReading,
    ) -> Self {
        let value = match reading.flux_estimate() {
            Some((value, uncertainty)) => NotebookValue::Estimate { value, uncertainty },
            None => NotebookValue::Unavailable {
                reason: unavailable_reason(reading),
            },
        };
        Self {
            family: family.into(),
            geometry: NotebookGeometry::Shell {
                shell_radius: scene.radius,
                probe_radius,
            },
            time,
            parameters,
            method: SHELL_METHOD.to_owned(),
            value,
            units: SHELL_UNITS.to_owned(),
        }
    }

    /// Whether the snapshot is structurally valid, finite and bounded.
    pub fn is_valid(&self) -> bool {
        if !self.time.is_finite() {
            return false;
        }
        let geometry_ok = match self.geometry {
            NotebookGeometry::Shell {
                shell_radius,
                probe_radius,
            } => {
                shell_radius.is_finite()
                    && shell_radius > 0.0
                    && probe_radius.is_finite()
                    && probe_radius > 0.0
            }
        };
        geometry_ok && self.value.is_valid() && self.parameters.validate().is_ok()
    }
}

fn unavailable_reason(reading: &GaussShellReading) -> String {
    match &reading.kind {
        GaussShellReadingKind::Excluded { reason } => reason.clone(),
        GaussShellReadingKind::Exterior { flux, .. }
        | GaussShellReadingKind::Interior { flux, .. } => match &flux.status {
            IntegralStatus::Unresolved { reason } | IntegralStatus::Singular { reason } => {
                reason.clone()
            }
            IntegralStatus::Estimate { .. } => {
                "The quadrature did not complete a finite estimate.".to_owned()
            }
        },
    }
}

/// One bounded notebook entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotebookEntry {
    /// Stable identifier, assigned once when the entry is pinned.
    pub id: String,
    /// The learner's prediction or note captured with the reading.
    pub prediction: String,
    /// A longer note captured with the reading.
    pub note: String,
    /// The immutable measurement.
    pub snapshot: MeasurementSnapshot,
    /// The whole scene that produced the reading, through the shipped
    /// versioned serializer.
    pub scene: SceneDocument,
}

impl Default for NotebookEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            prediction: String::new(),
            note: String::new(),
            snapshot: MeasurementSnapshot::default(),
            scene: SceneDocument::capture(
                // Deliberately invalid so an entry that omits its captured
                // scene is dropped by `sanitized` rather than silently loading
                // a default scene.
                "",
                ViewRecord::default(),
                ParameterSet::default(),
                SceneCamera::default(),
            ),
        }
    }
}

/// A bounded, ordered notebook. Older files load through the serde defaults and
/// are sanitized before use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notebook {
    /// The payload version; reloaded files are normalized to the current one.
    pub version: u32,
    /// Entries in stable pinned order.
    pub entries: Vec<NotebookEntry>,
}

impl Default for Notebook {
    fn default() -> Self {
        Self {
            version: NOTEBOOK_VERSION,
            entries: Vec::new(),
        }
    }
}

impl Notebook {
    /// An empty current-version notebook.
    pub fn new() -> Self {
        Self::default()
    }

    /// The entry with `id`, if present.
    pub fn get(&self, id: &str) -> Option<&NotebookEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Pin one entry, assigning the next stable `entry-N` id. Returns the id,
    /// or an error when the notebook is full or the capture is invalid.
    pub fn pin(
        &mut self,
        prediction: impl Into<String>,
        note: impl Into<String>,
        snapshot: MeasurementSnapshot,
        scene: SceneDocument,
    ) -> Result<String, String> {
        if self.entries.len() >= MAX_NOTEBOOK_ENTRIES {
            return Err(format!(
                "the notebook holds at most {MAX_NOTEBOOK_ENTRIES} entries"
            ));
        }
        if !snapshot.is_valid() {
            return Err("the measurement snapshot is not valid".to_owned());
        }
        scene
            .validate()
            .map_err(|error| format!("the captured scene is invalid: {error}"))?;

        let id = self.next_id();
        self.entries.push(NotebookEntry {
            id: id.clone(),
            prediction: prediction.into(),
            note: note.into(),
            snapshot,
            scene,
        });
        Ok(id)
    }

    /// Remove the entry with `id`, reporting whether one was removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != before
    }

    /// The next unused stable id.
    pub fn next_id(&self) -> String {
        let mut counter = self.entries.len() + 1;
        loop {
            let candidate = format!("entry-{counter}");
            if self.get(&candidate).is_none() {
                return candidate;
            }
            counter += 1;
        }
    }

    /// Append sanitized entries whose ids are new, up to the bound. Returns how
    /// many were added.
    pub fn append(&mut self, imported: Notebook) -> usize {
        let mut added = 0;
        for mut entry in imported.sanitized().entries {
            if self.entries.len() >= MAX_NOTEBOOK_ENTRIES {
                break;
            }
            if entry.id.trim().is_empty() {
                entry.id = self.next_id();
            }
            if self.get(&entry.id).is_some() {
                continue;
            }
            self.entries.push(entry);
            added += 1;
        }
        added
    }

    /// Replace all entries with a sanitized import.
    pub fn replace(&mut self, imported: Notebook) {
        let sanitized = imported.sanitized();
        self.version = NOTEBOOK_VERSION;
        self.entries = sanitized.entries;
    }

    /// Enforce the version, bound, unique ordered ids and structural validity.
    /// Corrupt entries are dropped rather than repaired into a different
    /// measurement.
    pub fn sanitized(mut self) -> Self {
        self.version = NOTEBOOK_VERSION;
        let mut ids = std::collections::HashSet::with_capacity(self.entries.len());
        let mut entries = Vec::with_capacity(self.entries.len().min(MAX_NOTEBOOK_ENTRIES));
        for mut entry in self.entries.drain(..) {
            if entries.len() == MAX_NOTEBOOK_ENTRIES {
                break;
            }
            if !entry.snapshot.is_valid() || entry.scene.validate().is_err() {
                continue;
            }
            if entry.id.trim().is_empty() {
                entry.id = format!("entry-{}", entries.len() + 1);
            }
            if ids.contains(&entry.id) {
                continue;
            }
            ids.insert(entry.id.clone());
            entries.push(entry);
        }
        self.entries = entries;
        self
    }

    /// Serialize the current, sanitized notebook as compact JSON.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.clone().sanitized())
            .map_err(|error| format!("invalid notebook JSON: {error}"))
    }

    /// Parse a notebook payload and sanitize it. Older files with absent fields
    /// load through the serde defaults; malformed JSON is rejected whole.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let notebook: Self = serde_json::from_str(text)
            .map_err(|error| format!("invalid notebook JSON: {error}"))?;
        Ok(notebook.sanitized())
    }
}

/// One label with the two displayed values that differ between entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedSetting {
    /// The setting name shown in the comparison.
    pub label: String,
    /// The first entry's displayed value.
    pub first: String,
    /// The second entry's displayed value.
    pub second: String,
}

/// The flux comparison for two estimate-carrying entries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FluxComparison {
    pub first: f64,
    pub first_uncertainty: f64,
    pub second: f64,
    pub second_uncertainty: f64,
    pub difference: f64,
    pub combined_uncertainty: f64,
    pub equal_within_uncertainty: bool,
}

/// The side-by-side comparison of two pinned entries.
#[derive(Debug, Clone, PartialEq)]
pub struct NotebookComparison {
    /// Every setting that differs, in a stable reading order.
    pub changes: Vec<ChangedSetting>,
    /// The measured-flux check when both entries carry estimates.
    pub flux: Option<FluxComparison>,
}

/// Compare two entries without mutating either. The result lists the family,
/// geometry (including both shell and probe radius), time, each differing
/// parameter, method, status and units, then the flux equality check.
pub fn compare(first: &NotebookEntry, second: &NotebookEntry) -> NotebookComparison {
    let mut changes = Vec::new();
    let a = &first.snapshot;
    let b = &second.snapshot;

    if a.family != b.family {
        changes.push(setting("Field family", a.family.label(), b.family.label()));
    }
    match (a.geometry, b.geometry) {
        (
            NotebookGeometry::Shell {
                shell_radius: first_shell,
                probe_radius: first_probe,
            },
            NotebookGeometry::Shell {
                shell_radius: second_shell,
                probe_radius: second_probe,
            },
        ) => {
            if first_shell.to_bits() != second_shell.to_bits() {
                changes.push(setting(
                    "Shell radius R",
                    &format!("{first_shell:.3}"),
                    &format!("{second_shell:.3}"),
                ));
            }
            if first_probe.to_bits() != second_probe.to_bits() {
                changes.push(setting(
                    "Probe radius r",
                    &format!("{first_probe:.3}"),
                    &format!("{second_probe:.3}"),
                ));
            }
        }
    }
    if a.time.to_bits() != b.time.to_bits() {
        changes.push(setting(
            "Time",
            &format!("{:.3} s", a.time),
            &format!("{:.3} s", b.time),
        ));
    }
    compare_parameters(&mut changes, &a.parameters, &b.parameters);
    if a.method != b.method {
        changes.push(setting("Method", &a.method, &b.method));
    }
    if a.value != b.value {
        changes.push(setting("Status", &a.value.text(), &b.value.text()));
    }
    if a.units != b.units {
        changes.push(setting("Units", &a.units, &b.units));
    }

    let flux = match (a.value.estimate(), b.value.estimate()) {
        (Some((first, first_uncertainty)), Some((second, second_uncertainty))) => {
            let difference = (first - second).abs();
            let combined_uncertainty = first_uncertainty + second_uncertainty;
            Some(FluxComparison {
                first,
                first_uncertainty,
                second,
                second_uncertainty,
                difference,
                combined_uncertainty,
                equal_within_uncertainty: difference <= combined_uncertainty + 1e-9,
            })
        }
        _ => None,
    };

    NotebookComparison { changes, flux }
}

fn setting(label: &str, first: &str, second: &str) -> ChangedSetting {
    ChangedSetting {
        label: label.to_owned(),
        first: first.to_owned(),
        second: second.to_owned(),
    }
}

fn compare_parameters(
    changes: &mut Vec<ChangedSetting>,
    first: &ParameterSet,
    second: &ParameterSet,
) {
    for parameter in &first.entries {
        let other = second
            .entries
            .iter()
            .find(|candidate| candidate.name == parameter.name);
        if other != Some(parameter) {
            changes.push(setting(
                &format!("Parameter {}", parameter.name),
                &parameter_text(parameter),
                &other.map_or_else(|| "absent".to_owned(), parameter_text),
            ));
        }
    }
    for parameter in &second.entries {
        if first
            .entries
            .iter()
            .any(|candidate| candidate.name == parameter.name)
        {
            continue;
        }
        changes.push(setting(
            &format!("Parameter {}", parameter.name),
            "absent",
            &parameter_text(parameter),
        ));
    }
}

fn parameter_text(parameter: &crate::parameters::Parameter) -> String {
    format!("{:.6}", parameter.value)
}
