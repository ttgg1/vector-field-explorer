//! Shared named parameters (issue #89 slice A): validated, durable records.
//!
//! A [`ParameterSet`] is a small, bounded list of named numbers with optional
//! bounds and a step. This slice defines the record, its validation rules and
//! the atomic commit seam only; formula parsing and UI controls are separate
//! approvals.

use serde::{Deserialize, Serialize};

/// A set never holds more than this many entries (bounded work/storage).
pub const MAX_PARAMETERS: usize = 32;
/// Names are at most this many ASCII bytes.
pub const MAX_NAME_LEN: usize = 32;
/// The only supported envelope version:
pub const PARAMETER_VERSION: u32 = 1;

/// Names that a formula may not bind as an editable parameter: the vector
/// coordinates and time, meval's constants, and every builtin function in the
/// locked `meval` 0.2.0 default context.
pub const RESERVED_NAMES: [&str; 29] = [
    "x", "y", "z", "t", "pi", "e", "sqrt", "exp", "ln", "abs", "sin", "cos", "tan", "asin", "acos",
    "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "floor", "ceil", "round", "signum",
    "atan2", "max", "min",
];

/// One named value with an optional inclusive range and a positive step.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Parameter {
    pub name: String,
    pub value: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: f64,
}

/// The durable, versioned list of named parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParameterSet {
    pub version: u32,
    pub entries: Vec<Parameter>,
}

impl Default for ParameterSet {
    /// The stored default is version 1 with no entries.
    fn default() -> Self {
        Self {
            version: PARAMETER_VERSION,
            entries: Vec::new(),
        }
    }
}

impl ParameterSet {
    /// Validate the whole set before it may replace the committed one.
    ///
    /// Errors name the offending parameter and the rule that failed.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PARAMETER_VERSION {
            return Err(format!(
                "parameter set version {} is unsupported; expected version {PARAMETER_VERSION}",
                self.version
            ));
        }
        if self.entries.len() > MAX_PARAMETERS {
            return Err(format!(
                "parameter set has {} entries; at most {MAX_PARAMETERS} are allowed",
                self.entries.len()
            ));
        }
        let mut seen = std::collections::HashSet::with_capacity(self.entries.len());
        for entry in &self.entries {
            validate_name(&entry.name)?;
            if RESERVED_NAMES.contains(&entry.name.as_str()) {
                return Err(format!(
                    "parameter \"{}\": name is reserved by the formula environment",
                    entry.name
                ));
            }
            if !seen.insert(entry.name.as_str()) {
                return Err(format!(
                    "parameter \"{}\": duplicate name; parameter names must be unique",
                    entry.name
                ));
            }
            validate_values(entry)?;
        }
        Ok(())
    }

    /// The committed value for `name`, if the set holds it.
    pub fn value(&self, name: &str) -> Option<f64> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.value)
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return Err("parameter name must not be empty".to_string());
    }
    if bytes.len() > MAX_NAME_LEN {
        return Err(format!(
            "parameter name \"{name}\": {} ASCII bytes exceeds the {MAX_NAME_LEN}-byte limit",
            bytes.len()
        ));
    }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return Err(format!(
            "parameter name \"{name}\": must start with an ASCII letter or underscore"
        ));
    }
    if !bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return Err(format!(
            "parameter name \"{name}\": may contain only ASCII letters, digits and underscores"
        ));
    }
    Ok(())
}

fn validate_values(entry: &Parameter) -> Result<(), String> {
    let name = &entry.name;
    if !entry.value.is_finite() {
        return Err(format!("parameter \"{name}\": value must be finite"));
    }
    if !entry.step.is_finite() || entry.step <= 0.0 {
        return Err(format!(
            "parameter \"{name}\": step must be finite and greater than zero"
        ));
    }
    if entry.min.is_some_and(|min| !min.is_finite()) {
        return Err(format!("parameter \"{name}\": min must be finite"));
    }
    if entry.max.is_some_and(|max| !max.is_finite()) {
        return Err(format!("parameter \"{name}\": max must be finite"));
    }
    if let (Some(min), Some(max)) = (entry.min, entry.max)
        && min > max
    {
        return Err(format!(
            "parameter \"{name}\": min ({min}) must not exceed max ({max})"
        ));
    }
    if let Some(min) = entry.min
        && entry.value < min
    {
        return Err(format!(
            "parameter \"{name}\": value {} is below min {min}",
            entry.value
        ));
    }
    if let Some(max) = entry.max
        && entry.value > max
    {
        return Err(format!(
            "parameter \"{name}\": value {} is above max {max}",
            entry.value
        ));
    }
    Ok(())
}
