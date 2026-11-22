//! Pure pin identities, arrangement validation and the durable pin preference.
//!
//! A pin is a *reference* to one existing control or readout, never a copy of
//! its value and never part of the physical document. The active pin order and
//! the named arrangements are workspace preferences; the model, the probe
//! geometry and every report stay in their ordinary owners.

use serde::{Deserialize, Serialize};

/// Most individual items one working set can hold. The initial catalogue is
/// smaller; the bound leaves room for later readouts without an unbounded
/// preference.
pub const MAX_PIN_ITEMS: usize = 8;
/// Most named arrangements one workspace can store.
pub const MAX_PIN_ARRANGEMENTS: usize = 12;
/// Longest arrangement name, in UTF-8 bytes.
pub const MAX_PIN_NAME_BYTES: usize = 40;

/// One pinnable existing control or readout.
///
/// Tokens are the stable stored identity (`gauss_radius`, …). A missing or
/// unknown token is dropped on load instead of failing the settings envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinItem {
    /// The Maxwell/Gauss guided-investigation measuring radius `r`.
    GaussRadius,
    /// The finite flux reading at the measuring radius.
    GaussFlux,
    /// The Maxwell hollow-sphere shell radius `R`.
    GaussShellRadius,
    /// Whether the colour scale auto-fits (`Auto`) or is held (`Locked`).
    GaussScaleLock,
    /// The active induction scene's frequency readout.
    InductionFrequency,
    /// The shared clock time while induction is active.
    InductionTime,
    /// The committed induction axis while induction is active.
    InductionOrientation,
}

impl PinItem {
    /// The complete initial catalogue, in tray order.
    pub const ALL: [PinItem; 7] = [
        PinItem::GaussRadius,
        PinItem::GaussFlux,
        PinItem::GaussShellRadius,
        PinItem::GaussScaleLock,
        PinItem::InductionFrequency,
        PinItem::InductionTime,
        PinItem::InductionOrientation,
    ];

    /// The stable stored token.
    pub const fn id(self) -> &'static str {
        match self {
            PinItem::GaussRadius => "gauss_radius",
            PinItem::GaussFlux => "gauss_flux",
            PinItem::GaussShellRadius => "gauss_shell_radius",
            PinItem::GaussScaleLock => "gauss_scale_lock",
            PinItem::InductionFrequency => "induction_frequency",
            PinItem::InductionTime => "induction_time",
            PinItem::InductionOrientation => "induction_orientation",
        }
    }

    /// The learner-facing label.
    pub const fn label(self) -> &'static str {
        match self {
            PinItem::GaussRadius => "Gauss measuring radius",
            PinItem::GaussFlux => "Gauss flux",
            PinItem::GaussShellRadius => "Gauss shell radius",
            PinItem::GaussScaleLock => "Colour scale lock",
            PinItem::InductionFrequency => "Induction frequency",
            PinItem::InductionTime => "Induction time",
            PinItem::InductionOrientation => "Induction axis",
        }
    }

    /// A short label for compact tray rows and buttons.
    pub const fn short_label(self) -> &'static str {
        match self {
            PinItem::GaussRadius => "radius",
            PinItem::GaussFlux => "flux",
            PinItem::GaussShellRadius => "shell R",
            PinItem::GaussScaleLock => "scale",
            PinItem::InductionFrequency => "frequency",
            PinItem::InductionTime => "time",
            PinItem::InductionOrientation => "axis",
        }
    }

    /// Whether the item is an editable control or a read-only readout.
    pub const fn is_control(self) -> bool {
        matches!(self, PinItem::GaussRadius)
    }

    /// Decode one stored token. Unknown text is not an error; the caller drops
    /// it so a hand-edited preference cannot fail the envelope.
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.id() == token)
    }
}

/// A bounded, ordered set of pinned items with no duplicates.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PinSet {
    /// Pinned items in visible order.
    #[serde(default, deserialize_with = "deserialize_items")]
    pub items: Vec<PinItem>,
}

impl PinSet {
    /// Whether `item` is already pinned.
    pub fn contains(&self, item: PinItem) -> bool {
        self.items.contains(&item)
    }

    /// Whether the set has no pins.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Append `item` once. Returns `false` when it is already pinned or the set
    /// is at [`MAX_PIN_ITEMS`].
    pub fn pin(&mut self, item: PinItem) -> bool {
        if self.contains(item) || self.items.len() >= MAX_PIN_ITEMS {
            return false;
        }
        self.items.push(item);
        true
    }

    /// Remove `item`. Returns whether it was present.
    pub fn unpin(&mut self, item: PinItem) -> bool {
        let before = self.items.len();
        self.items.retain(|pinned| *pinned != item);
        self.items.len() != before
    }

    /// Swap `index` with its predecessor. Returns whether the order changed.
    pub fn move_up(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.items.len() {
            return false;
        }
        self.items.swap(index - 1, index);
        true
    }

    /// Swap `index` with its successor. Returns whether the order changed.
    pub fn move_down(&mut self, index: usize) -> bool {
        if index + 1 >= self.items.len() {
            return false;
        }
        self.items.swap(index, index + 1);
        true
    }

    /// Enforce the bound, drop duplicates keeping the first occurrence and
    /// preserve order. Total.
    pub fn sanitized(mut self) -> Self {
        let mut seen = Vec::with_capacity(self.items.len().min(MAX_PIN_ITEMS));
        for item in self.items.drain(..) {
            if seen.contains(&item) {
                continue;
            }
            if seen.len() == MAX_PIN_ITEMS {
                break;
            }
            seen.push(item);
        }
        self.items = seen;
        self
    }

    /// Reject duplicates or an excess count.
    pub fn validate(&self) -> Result<(), String> {
        if self.items.len() > MAX_PIN_ITEMS {
            return Err(format!(
                "a pin set holds at most {MAX_PIN_ITEMS} items, got {}",
                self.items.len()
            ));
        }
        for (index, item) in self.items.iter().enumerate() {
            if self.items[..index].contains(item) {
                return Err(format!("pin {:?} appears more than once", item.id()));
            }
        }
        Ok(())
    }
}

/// One named, restorable pin order. The items reference controls/readouts; no
/// physical value or model state is stored here.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PinArrangement {
    /// Trimmed, non-empty, at most [`MAX_PIN_NAME_BYTES`] UTF-8 bytes.
    pub name: String,
    /// Active items captured when the arrangement was saved.
    #[serde(default, deserialize_with = "deserialize_items")]
    pub items: Vec<PinItem>,
}

impl PinArrangement {
    /// Clamp the name and repair the item list. Total.
    pub fn sanitized(self) -> Self {
        Self {
            name: sanitize_name(&self.name),
            items: PinSet { items: self.items }.sanitized().items,
        }
    }

    /// Reject an empty, overlong or untrimmed name or an invalid item list.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("a pin arrangement needs a non-empty name".to_owned());
        }
        if self.name != self.name.trim() {
            return Err("a pin arrangement name must be trimmed".to_owned());
        }
        if self.name.len() > MAX_PIN_NAME_BYTES {
            return Err(format!(
                "a pin arrangement name holds at most {MAX_PIN_NAME_BYTES} bytes"
            ));
        }
        PinSet {
            items: self.items.clone(),
        }
        .validate()
    }
}

/// The durable pin preference: the active working set plus named arrangements.
///
/// This never carries model state, probe geometry or sampled values, so
/// restoring it cannot mutate or stale the physical model.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PinUiRecord {
    /// The visible working set.
    pub active: PinSet,
    /// Named arrangements in creation order.
    #[serde(default, deserialize_with = "deserialize_saved")]
    pub saved: Vec<PinArrangement>,
}

impl PinUiRecord {
    /// Pin one item into the visible set.
    pub fn pin(&mut self, item: PinItem) -> bool {
        self.active.pin(item)
    }

    /// Remove one item from the visible set.
    pub fn unpin(&mut self, item: PinItem) -> bool {
        self.active.unpin(item)
    }

    /// Move one visible item toward the front.
    pub fn move_up(&mut self, index: usize) -> bool {
        self.active.move_up(index)
    }

    /// Move one visible item toward the back.
    pub fn move_down(&mut self, index: usize) -> bool {
        self.active.move_down(index)
    }

    /// Save the current item order under a sanitized name, replacing an
    /// arrangement with the same name. Returns the stored name.
    pub fn save_arrangement(&mut self, name: &str) -> Result<String, String> {
        let name = sanitize_name(name);
        if name.is_empty() {
            return Err("Enter a name for the pin arrangement.".to_owned());
        }
        let arrangement = PinArrangement {
            name: name.clone(),
            items: self.active.items.clone(),
        };
        if let Some(existing) = self.saved.iter_mut().find(|saved| saved.name == name) {
            *existing = arrangement;
            return Ok(name);
        }
        if self.saved.len() >= MAX_PIN_ARRANGEMENTS {
            return Err(format!(
                "At most {MAX_PIN_ARRANGEMENTS} pin arrangements can be saved."
            ));
        }
        self.saved.push(arrangement);
        Ok(name)
    }

    /// Replace the visible order with a saved arrangement. Returns whether a
    /// matching, non-empty-name arrangement existed.
    pub fn restore_arrangement(&mut self, name: &str) -> bool {
        let name = sanitize_name(name);
        let Some(arrangement) = self.saved.iter().find(|saved| saved.name == name) else {
            return false;
        };
        self.active = PinSet {
            items: arrangement.items.clone(),
        };
        true
    }

    /// Remove one named arrangement.
    pub fn delete_arrangement(&mut self, name: &str) -> bool {
        let before = self.saved.len();
        self.saved.retain(|saved| saved.name != name);
        self.saved.len() != before
    }

    /// Repair every stored field: drop unknown/duplicate items, empty names,
    /// duplicate arrangement names and excess entries. Total.
    pub fn sanitized(mut self) -> Self {
        let active = self.active.sanitized();
        let mut names = Vec::with_capacity(self.saved.len().min(MAX_PIN_ARRANGEMENTS));
        let mut saved = Vec::with_capacity(self.saved.len().min(MAX_PIN_ARRANGEMENTS));
        for arrangement in self.saved.drain(..) {
            let arrangement = arrangement.sanitized();
            if arrangement.name.is_empty() {
                continue;
            }
            if names.contains(&arrangement.name) {
                continue;
            }
            if saved.len() == MAX_PIN_ARRANGEMENTS {
                break;
            }
            names.push(arrangement.name.clone());
            saved.push(arrangement);
        }
        self.active = active;
        self.saved = saved;
        self
    }
}

/// Trim surrounding whitespace and truncate at a character boundary to
/// [`MAX_PIN_NAME_BYTES`]. Total.
pub fn sanitize_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.len() <= MAX_PIN_NAME_BYTES {
        return trimmed.to_owned();
    }
    let mut end = MAX_PIN_NAME_BYTES;
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].trim_end().to_owned()
}

/// Decode pin items leniently: unknown tokens and non-string entries are
/// dropped, and a non-sequence value becomes an empty set.
fn deserialize_items<'de, D>(deserializer: D) -> Result<Vec<PinItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Token(String),
        Invalid(serde::de::IgnoredAny),
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Wire {
        Items(Vec<Entry>),
        Invalid(serde::de::IgnoredAny),
    }

    let items = match Wire::deserialize(deserializer)? {
        Wire::Items(entries) => entries
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Token(token) => PinItem::from_token(&token),
                Entry::Invalid(_) => None,
            })
            .collect(),
        Wire::Invalid(_) => Vec::new(),
    };
    Ok(PinSet { items }.sanitized().items)
}

/// Decode arrangements leniently: a malformed entry or a non-sequence value
/// cannot fail the whole settings envelope.
fn deserialize_saved<'de, D>(deserializer: D) -> Result<Vec<PinArrangement>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Arrangement(PinArrangement),
        Invalid(serde::de::IgnoredAny),
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Wire {
        Items(Vec<Entry>),
        Invalid(serde::de::IgnoredAny),
    }

    let saved = match Wire::deserialize(deserializer)? {
        Wire::Items(entries) => entries
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Arrangement(arrangement) => Some(arrangement),
                Entry::Invalid(_) => None,
            })
            .collect(),
        Wire::Invalid(_) => Vec::new(),
    };
    Ok(PinUiRecord {
        active: PinSet::default(),
        saved,
    }
    .sanitized()
    .saved)
}
