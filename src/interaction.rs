//! Pure interaction-profile model (issue #88 slice A1).
//!
//! An [`InteractionProfile`] bundles the one-chord-per-action binding table and
//! the camera-feel multipliers under a readable name. [`ProfileDocument`] is
//! the strict, versioned portable form and [`ProfileLibrary`] the bounded
//! collection of user profiles beside the built-in `Default`.
//!
//! This module is deliberately pure: it owns no UI, input routing, persistence,
//! `AppState`, renderer or camera constants.

use std::collections::{BTreeMap, HashSet};

use eframe::egui::Key;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The only interaction-profile document version this module writes or accepts.
pub const INTERACTION_FORMAT_VERSION: u32 = 1;

/// Largest accepted profile name, in UTF-8 bytes.
pub const MAX_PROFILE_NAME_BYTES: usize = 48;

/// Largest number of user profiles the library keeps.
pub const MAX_USER_PROFILES: usize = 16;

/// Largest number of remappable bindings: one chord per remappable action.
pub const MAX_BINDINGS: usize = 19;

/// Smallest accepted orbit-sensitivity or zoom-speed multiplier.
pub const MIN_CAMERA_MULTIPLIER: f32 = 0.25;

/// Largest accepted orbit-sensitivity or zoom-speed multiplier.
pub const MAX_CAMERA_MULTIPLIER: f32 = 4.0;

/// Name of the built-in profile; it is never stored as a user profile.
pub const DEFAULT_PROFILE_NAME: &str = "Default";

/// Fallback name for a sanitized record whose name is empty.
const FALLBACK_PROFILE_NAME: &str = "Untitled";

const REMAPPABLE_COUNT: usize = 19;

/// The logical modifier set of a [`KeyChord`].
///
/// `command` is `Mod`: Ctrl on Linux/Windows and Command on macOS, matching
/// egui's logical modifier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChordModifiers {
    /// `Mod` (Ctrl or Command).
    pub command: bool,
    /// Shift.
    pub shift: bool,
    /// Alt.
    pub alt: bool,
}

/// One logical key plus its modifier set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    /// The logical egui key.
    pub key: Key,
    /// The required modifiers.
    pub modifiers: ChordModifiers,
}

impl KeyChord {
    /// A compact, platform-independent label such as `Mod+Shift+A` or `F1`.
    ///
    /// Modifiers always appear in the order `Mod`, `Shift`, `Alt`, and `Mod`
    /// is spelled `Mod` rather than the platform key.
    pub fn label(&self) -> String {
        let mut label = String::new();
        if self.modifiers.command {
            label.push_str("Mod+");
        }
        if self.modifiers.shift {
            label.push_str("Shift+");
        }
        if self.modifiers.alt {
            label.push_str("Alt+");
        }
        label.push_str(self.key.name());
        label
    }

    /// Whether a web browser owns this chord, so it is refused while newly
    /// assigned: `Mod+W`, `Mod+T`, `Mod+N`, `Mod+Shift+W`, `Mod+Shift+I`,
    /// `Mod+Shift+J`, `Mod+Shift+K`, `Mod+Shift+C` and `Mod+Q`.
    ///
    /// The shipped `Mod+Q` and `Mod+Shift+C` defaults are grandfathered; see
    /// [`Bindings`].
    pub fn is_browser_reserved(&self) -> bool {
        if !self.modifiers.command || self.modifiers.alt {
            return false;
        }
        if !self.modifiers.shift {
            return matches!(self.key, Key::W | Key::T | Key::N | Key::Q);
        }
        matches!(self.key, Key::W | Key::I | Key::J | Key::K | Key::C)
    }

    /// Chords reserved by the application and never remappable: `Escape`,
    /// `Mod+Z` (undo) and `Mod+Shift+Z` (redo).
    fn is_reserved_chord(&self) -> bool {
        self.key == Key::Escape
            || (self.modifiers.command && !self.modifiers.alt && self.key == Key::Z)
    }
}

impl Serialize for KeyChord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Chord<'a> {
            key: &'a str,
            modifiers: ChordModifiers,
        }

        Chord {
            key: self.key.name(),
            modifiers: self.modifiers,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KeyChord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Chord {
            key: String,
            modifiers: ChordModifiers,
        }

        let chord = Chord::deserialize(deserializer)?;
        let key = Key::from_name(&chord.key)
            .ok_or_else(|| D::Error::custom(format!("unknown key {:?}", chord.key)))?;
        Ok(Self {
            key,
            modifiers: chord.modifiers,
        })
    }
}

/// Every action a profile describes: the nineteen remappable actions and the
/// three read-only reserved ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionAction {
    // Panels and surfaces.
    ToggleControls,
    TogglePresets,
    ToggleAppearance,
    ToggleInteraction,
    ToggleHelp,
    // View.
    ResetView,
    FitView,
    ZoomIn,
    ZoomOut,
    ViewLeft,
    ViewRight,
    ViewUp,
    ViewDown,
    // Simulation.
    PlayPause,
    // Tools.
    SeedStreamline,
    AddParticles,
    // Files.
    Screenshot,
    CopyFormula,
    Quit,
    // Reserved, read-only.
    Dismiss,
    Undo,
    Redo,
}

impl InteractionAction {
    /// Every action, remappable ones first and the reserved ones last.
    pub const ALL: [Self; 22] = [
        Self::ToggleControls,
        Self::TogglePresets,
        Self::ToggleAppearance,
        Self::ToggleInteraction,
        Self::ToggleHelp,
        Self::ResetView,
        Self::FitView,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ViewLeft,
        Self::ViewRight,
        Self::ViewUp,
        Self::ViewDown,
        Self::PlayPause,
        Self::SeedStreamline,
        Self::AddParticles,
        Self::Screenshot,
        Self::CopyFormula,
        Self::Quit,
        Self::Dismiss,
        Self::Undo,
        Self::Redo,
    ];

    /// The nineteen actions a profile may bind; reserved actions are excluded,
    /// and fly toggle stays on the physical-key path.
    pub const REMAPPABLE: [Self; 19] = [
        Self::ToggleControls,
        Self::TogglePresets,
        Self::ToggleAppearance,
        Self::ToggleInteraction,
        Self::ToggleHelp,
        Self::ResetView,
        Self::FitView,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ViewLeft,
        Self::ViewRight,
        Self::ViewUp,
        Self::ViewDown,
        Self::PlayPause,
        Self::SeedStreamline,
        Self::AddParticles,
        Self::Screenshot,
        Self::CopyFormula,
        Self::Quit,
    ];

    /// A human-readable label for the action row.
    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleControls => "Controls panel",
            Self::TogglePresets => "Presets panel",
            Self::ToggleAppearance => "Appearance panel",
            Self::ToggleInteraction => "Interaction panel",
            Self::ToggleHelp => "Help",
            Self::ResetView => "Reset view",
            Self::FitView => "Fit view",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::ViewLeft => "View left",
            Self::ViewRight => "View right",
            Self::ViewUp => "View up",
            Self::ViewDown => "View down",
            Self::PlayPause => "Play/pause",
            Self::SeedStreamline => "Seed streamline",
            Self::AddParticles => "Add particles",
            Self::Screenshot => "Screenshot",
            Self::CopyFormula => "Copy formula",
            Self::Quit => "Quit",
            Self::Dismiss => "Dismiss / cancel",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
        }
    }

    /// Whether the action is reserved and therefore read-only.
    pub fn is_reserved(self) -> bool {
        matches!(self, Self::Dismiss | Self::Undo | Self::Redo)
    }

    /// Whether the action keeps firing while a text field has focus. Today
    /// only Quit is global.
    pub fn is_global(self) -> bool {
        matches!(self, Self::Quit)
    }

    /// The stable JSON identifier, or `None` for a reserved action.
    fn id(self) -> Option<&'static str> {
        Some(match self {
            Self::ToggleControls => "toggle_controls",
            Self::TogglePresets => "toggle_presets",
            Self::ToggleAppearance => "toggle_appearance",
            Self::ToggleInteraction => "toggle_interaction",
            Self::ToggleHelp => "toggle_help",
            Self::ResetView => "reset_view",
            Self::FitView => "fit_view",
            Self::ZoomIn => "zoom_in",
            Self::ZoomOut => "zoom_out",
            Self::ViewLeft => "view_left",
            Self::ViewRight => "view_right",
            Self::ViewUp => "view_up",
            Self::ViewDown => "view_down",
            Self::PlayPause => "play_pause",
            Self::SeedStreamline => "seed_streamline",
            Self::AddParticles => "add_particles",
            Self::Screenshot => "screenshot",
            Self::CopyFormula => "copy_formula",
            Self::Quit => "quit",
            Self::Dismiss | Self::Undo | Self::Redo => return None,
        })
    }

    /// Parse a stable JSON identifier.
    fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "toggle_controls" => Self::ToggleControls,
            "toggle_presets" => Self::TogglePresets,
            "toggle_appearance" => Self::ToggleAppearance,
            "toggle_interaction" => Self::ToggleInteraction,
            "toggle_help" => Self::ToggleHelp,
            "reset_view" => Self::ResetView,
            "fit_view" => Self::FitView,
            "zoom_in" => Self::ZoomIn,
            "zoom_out" => Self::ZoomOut,
            "view_left" => Self::ViewLeft,
            "view_right" => Self::ViewRight,
            "view_up" => Self::ViewUp,
            "view_down" => Self::ViewDown,
            "play_pause" => Self::PlayPause,
            "seed_streamline" => Self::SeedStreamline,
            "add_particles" => Self::AddParticles,
            "screenshot" => Self::Screenshot,
            "copy_formula" => Self::CopyFormula,
            "quit" => Self::Quit,
            _ => return None,
        })
    }

    fn remappable_index(self) -> Option<usize> {
        Self::REMAPPABLE
            .iter()
            .position(|candidate| *candidate == self)
    }
}

/// The documented default chord for a remappable action.
fn default_chord(action: InteractionAction) -> KeyChord {
    let command = |key| KeyChord {
        key,
        modifiers: ChordModifiers {
            command: true,
            shift: false,
            alt: false,
        },
    };
    let shifted = |key| KeyChord {
        key,
        modifiers: ChordModifiers {
            command: true,
            shift: true,
            alt: false,
        },
    };
    let plain = |key| KeyChord {
        key,
        modifiers: ChordModifiers::default(),
    };

    match action {
        InteractionAction::ToggleControls => command(Key::B),
        InteractionAction::TogglePresets => command(Key::P),
        InteractionAction::ToggleAppearance => shifted(Key::A),
        InteractionAction::ToggleInteraction => shifted(Key::L),
        InteractionAction::ToggleHelp => plain(Key::F1),
        InteractionAction::ResetView => plain(Key::R),
        InteractionAction::FitView => plain(Key::F),
        // Wiring also accepts `=`; the binding keeps the documented `Plus`.
        InteractionAction::ZoomIn => plain(Key::Plus),
        InteractionAction::ZoomOut => plain(Key::Minus),
        InteractionAction::ViewLeft => plain(Key::ArrowLeft),
        InteractionAction::ViewRight => plain(Key::ArrowRight),
        InteractionAction::ViewUp => plain(Key::ArrowUp),
        InteractionAction::ViewDown => plain(Key::ArrowDown),
        InteractionAction::PlayPause => plain(Key::Space),
        InteractionAction::SeedStreamline => plain(Key::S),
        InteractionAction::AddParticles => plain(Key::P),
        InteractionAction::Screenshot => shifted(Key::S),
        InteractionAction::CopyFormula => shifted(Key::C),
        InteractionAction::Quit => command(Key::Q),
        InteractionAction::Dismiss | InteractionAction::Undo | InteractionAction::Redo => {
            unreachable!("reserved actions have no default chord")
        }
    }
}

/// The shipped default that owns a grandfathered chord, if any.
///
/// `Mod+Q`, `Mod+Shift+C` and `Mod+Shift+S` shipped before this contract. They
/// stay bindable to their original action (including on restore) but are
/// refused for any other action.
fn grandfathered_owner(chord: KeyChord) -> Option<InteractionAction> {
    if !chord.modifiers.command || chord.modifiers.alt {
        return None;
    }
    if !chord.modifiers.shift {
        return (chord.key == Key::Q).then_some(InteractionAction::Quit);
    }
    match chord.key {
        Key::C => Some(InteractionAction::CopyFormula),
        Key::S => Some(InteractionAction::Screenshot),
        _ => None,
    }
}

/// Whether `chord` may bind `action`: reserved and browser chords are refused,
/// except a grandfathered default chord bound to its original action.
fn chord_allowed_for(action: InteractionAction, chord: KeyChord) -> bool {
    if chord.is_reserved_chord() {
        return false;
    }
    match grandfathered_owner(chord) {
        Some(owner) => owner == action,
        None => !chord.is_browser_reserved(),
    }
}

/// Why a binding edit was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindError {
    /// The action or chord is reserved.
    Reserved,
    /// A browser owns the chord.
    BrowserReserved,
    /// Another action already owns the chord.
    Conflict(InteractionAction),
    /// The action is not part of the binding table.
    Unknown,
}

/// The one-chord-per-remappable-action table.
///
/// A freshly parsed record may be partial; [`Bindings::validate`] rejects that
/// and [`InteractionProfile::sanitized`] repairs it. A default table is fully
/// bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    chords: [Option<KeyChord>; REMAPPABLE_COUNT],
}

impl Default for Bindings {
    fn default() -> Self {
        Self::default_bindings()
    }
}

impl Bindings {
    /// The documented default table.
    pub fn default_bindings() -> Self {
        let mut bindings = Self::empty();
        for action in InteractionAction::REMAPPABLE {
            bindings.set(action, default_chord(action));
        }
        bindings
    }

    /// The chord bound to `action`, or `None` for a reserved or unbound action.
    pub fn chord(&self, action: InteractionAction) -> Option<KeyChord> {
        action
            .remappable_index()
            .and_then(|index| self.chords[index])
    }

    /// The remappable action that owns `chord`, if any.
    pub fn owner_of(&self, chord: KeyChord) -> Option<InteractionAction> {
        InteractionAction::REMAPPABLE
            .into_iter()
            .find(|action| self.chord(*action) == Some(chord))
    }

    /// Bind `chord` to `action`, refusing reserved actions and chords, browser
    /// chords and chords owned by another action. A refusal changes nothing.
    pub fn assign(&mut self, action: InteractionAction, chord: KeyChord) -> Result<(), BindError> {
        self.check_assignable(action, chord)?;
        self.set(action, chord);
        Ok(())
    }

    /// Bind `chord` to `action`. If another action owns the chord, the two
    /// chords are swapped in one step so both actions stay bound and no
    /// duplicate exists. A free chord is simply assigned.
    pub fn replace(&mut self, action: InteractionAction, chord: KeyChord) -> Result<(), BindError> {
        Self::check_target(action, chord)?;
        match self.owner_of(chord) {
            Some(owner) if owner != action => {
                let Some(replacement) = self.chord(action) else {
                    // A partial table has nothing to swap; refuse rather than
                    // create a duplicate.
                    return Err(BindError::Conflict(owner));
                };
                self.set(owner, replacement);
                self.set(action, chord);
            }
            Some(_) | None => self.set(action, chord),
        }
        Ok(())
    }

    /// Restore `action`'s documented default chord. Reserved actions are left
    /// untouched.
    pub fn reset_action(&mut self, action: InteractionAction) {
        if action.remappable_index().is_some() {
            self.set(action, default_chord(action));
        }
    }

    /// Restore every documented default chord.
    pub fn reset_all(&mut self) {
        *self = Self::default_bindings();
    }

    /// Prove the table is complete, duplicate-free and free of reserved and
    /// browser-owned chords.
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = HashSet::with_capacity(REMAPPABLE_COUNT);
        for action in InteractionAction::REMAPPABLE {
            let Some(chord) = self.chord(action) else {
                return Err(format!("{} is not bound", action.label()));
            };
            if chord.is_reserved_chord() {
                return Err(format!(
                    "{} uses the reserved chord {}",
                    action.label(),
                    chord.label()
                ));
            }
            if !chord_allowed_for(action, chord) {
                return Err(format!(
                    "{} uses the browser-owned chord {}",
                    action.label(),
                    chord.label()
                ));
            }
            if !seen.insert(chord) {
                return Err(format!(
                    "{} is bound to more than one action",
                    chord.label()
                ));
            }
        }
        Ok(())
    }

    fn check_assignable(
        &self,
        action: InteractionAction,
        chord: KeyChord,
    ) -> Result<(), BindError> {
        Self::check_target(action, chord)?;
        match self.owner_of(chord) {
            Some(owner) if owner != action => Err(BindError::Conflict(owner)),
            _ => Ok(()),
        }
    }

    fn check_target(action: InteractionAction, chord: KeyChord) -> Result<(), BindError> {
        if action.remappable_index().is_none() {
            return Err(if action.is_reserved() {
                BindError::Reserved
            } else {
                BindError::Unknown
            });
        }
        if chord.is_reserved_chord() {
            return Err(BindError::Reserved);
        }
        if !chord_allowed_for(action, chord) {
            return Err(BindError::BrowserReserved);
        }
        Ok(())
    }

    fn empty() -> Self {
        Self {
            chords: [None; REMAPPABLE_COUNT],
        }
    }

    fn set(&mut self, action: InteractionAction, chord: KeyChord) {
        if let Some(index) = action.remappable_index() {
            self.chords[index] = Some(chord);
        }
    }

    /// Repair a stored table: missing, reserved and browser-owned entries
    /// return to their defaults, and duplicate entries are resolved without
    /// ever leaking a duplicate. The result always validates.
    fn sanitized(self) -> Self {
        let mut bindings = self;
        for action in InteractionAction::REMAPPABLE {
            let invalid = match bindings.chord(action) {
                Some(chord) => !chord_allowed_for(action, chord),
                None => true,
            };
            if invalid {
                bindings.set(action, default_chord(action));
            }
        }

        let mut seen = HashSet::with_capacity(REMAPPABLE_COUNT);
        for action in InteractionAction::REMAPPABLE {
            let chord = bindings
                .chord(action)
                .expect("every action was repaired above");
            if !seen.insert(chord) {
                bindings.set(action, default_chord(action));
            }
        }

        // A default restored above can still collide with an earlier custom
        // chord; fall back to the fully default table in that rare case.
        let mut seen = HashSet::with_capacity(REMAPPABLE_COUNT);
        let duplicated = InteractionAction::REMAPPABLE
            .into_iter()
            .filter_map(|action| bindings.chord(action))
            .any(|chord| !seen.insert(chord));
        if duplicated {
            bindings = Self::default_bindings();
        }
        bindings
    }
}

impl Serialize for Bindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = BTreeMap::new();
        for action in InteractionAction::REMAPPABLE {
            if let (Some(id), Some(chord)) = (action.id(), self.chord(action)) {
                map.insert(id, chord);
            }
        }
        map.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map = BTreeMap::<String, KeyChord>::deserialize(deserializer)?;
        let mut bindings = Self::empty();
        for (id, chord) in map {
            let action = InteractionAction::from_id(&id)
                .ok_or_else(|| D::Error::custom(format!("unknown action {id:?}")))?;
            let index = action.remappable_index().ok_or_else(|| {
                D::Error::custom(format!("reserved action {id:?} is not bindable"))
            })?;
            bindings.chords[index] = Some(chord);
        }
        Ok(bindings)
    }
}

/// Camera sensitivity multipliers and invert flags.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraFeel {
    /// Orbit sensitivity multiplier in `0.25..=4.0`; default `1.0`.
    #[serde(default = "default_multiplier")]
    pub orbit_sensitivity: f32,
    /// Wheel zoom speed multiplier in `0.25..=4.0`; default `1.0`.
    #[serde(default = "default_multiplier")]
    pub zoom_speed: f32,
    /// Flip horizontal orbit/look input.
    #[serde(default)]
    pub invert_yaw: bool,
    /// Flip vertical orbit/look input.
    #[serde(default)]
    pub invert_pitch: bool,
    /// Flip the wheel-zoom direction.
    #[serde(default)]
    pub invert_zoom: bool,
}

fn default_multiplier() -> f32 {
    1.0
}

impl Default for CameraFeel {
    fn default() -> Self {
        Self {
            orbit_sensitivity: 1.0,
            zoom_speed: 1.0,
            invert_yaw: false,
            invert_pitch: false,
            invert_zoom: false,
        }
    }
}

impl CameraFeel {
    /// Reject non-finite or out-of-range multipliers.
    pub fn validate(&self) -> Result<(), String> {
        validate_multiplier(self.orbit_sensitivity, "orbit sensitivity")?;
        validate_multiplier(self.zoom_speed, "zoom speed")
    }

    /// Clamp multipliers and reset non-finite ones to `1.0`. Total.
    pub fn sanitized(self) -> Self {
        Self {
            orbit_sensitivity: sanitize_multiplier(self.orbit_sensitivity),
            zoom_speed: sanitize_multiplier(self.zoom_speed),
            ..self
        }
    }
}

fn validate_multiplier(value: f32, name: &str) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite; got {value}"));
    }
    if !(MIN_CAMERA_MULTIPLIER..=MAX_CAMERA_MULTIPLIER).contains(&value) {
        return Err(format!(
            "{name} {value} is outside {MIN_CAMERA_MULTIPLIER}..={MAX_CAMERA_MULTIPLIER}"
        ));
    }
    Ok(())
}

fn sanitize_multiplier(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(MIN_CAMERA_MULTIPLIER, MAX_CAMERA_MULTIPLIER)
    } else {
        1.0
    }
}

/// A named binding table plus camera feel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionProfile {
    /// Trimmed, non-empty, at most [`MAX_PROFILE_NAME_BYTES`] UTF-8 bytes.
    pub name: String,
    /// One chord per remappable action.
    #[serde(default)]
    pub bindings: Bindings,
    /// Multipliers and invert flags.
    #[serde(default)]
    pub camera_feel: CameraFeel,
}

impl Default for InteractionProfile {
    fn default() -> Self {
        Self {
            name: DEFAULT_PROFILE_NAME.to_owned(),
            bindings: Bindings::default_bindings(),
            camera_feel: CameraFeel::default(),
        }
    }
}

impl InteractionProfile {
    /// Reject the first invalid name, binding or camera-feel value.
    pub fn validate(&self) -> Result<(), String> {
        validate_name(&self.name)?;
        self.bindings.validate()?;
        self.camera_feel.validate()
    }

    /// Repair a stored record: trim/truncate the name, repair every binding and
    /// sanitize camera feel. The output always validates.
    pub fn sanitized(self) -> Self {
        Self {
            name: sanitize_name(&self.name),
            bindings: self.bindings.sanitized(),
            camera_feel: self.camera_feel.sanitized(),
        }
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("profile name must not be empty".to_owned());
    }
    if name.trim() != name {
        return Err("profile name must be trimmed".to_owned());
    }
    if name.len() > MAX_PROFILE_NAME_BYTES {
        return Err(format!(
            "profile name is {} bytes; the limit is {MAX_PROFILE_NAME_BYTES}",
            name.len()
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("profile name must not contain control characters".to_owned());
    }
    Ok(())
}

fn sanitize_name(name: &str) -> String {
    let mut cleaned: String = name
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    cleaned = cleaned.trim().to_owned();
    if cleaned.is_empty() {
        return FALLBACK_PROFILE_NAME.to_owned();
    }
    if cleaned.len() > MAX_PROFILE_NAME_BYTES {
        let mut end = MAX_PROFILE_NAME_BYTES;
        while !cleaned.is_char_boundary(end) {
            end -= 1;
        }
        cleaned.truncate(end);
        cleaned = cleaned.trim_end().to_owned();
        if cleaned.is_empty() {
            return FALLBACK_PROFILE_NAME.to_owned();
        }
    }
    cleaned
}

/// A strict, versioned JSON document holding interaction profiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDocument {
    /// Document format version; must be [`INTERACTION_FORMAT_VERSION`].
    pub version: u32,
    /// Profiles in document order.
    pub profiles: Vec<InteractionProfile>,
}

impl ProfileDocument {
    fn validate(&self) -> Result<(), String> {
        if self.version != INTERACTION_FORMAT_VERSION {
            return Err(format!(
                "interaction profile version {} is not supported; expected version {}",
                self.version, INTERACTION_FORMAT_VERSION
            ));
        }
        let mut seen = HashSet::with_capacity(self.profiles.len());
        for profile in &self.profiles {
            profile
                .validate()
                .map_err(|error| format!("invalid profile {:?}: {error}", profile.name))?;
            if !seen.insert(profile.name.as_str()) {
                return Err(format!(
                    "duplicate profile name {:?}; profile names must be unique",
                    profile.name
                ));
            }
        }
        Ok(())
    }

    /// Serialize a validated document as compact JSON with pinned field order:
    /// `version`, `profiles`; per profile `name`, `bindings`, `camera_feel`;
    /// bindings as an action-keyed map in action-id order; per chord `key`,
    /// `modifiers`; per modifier `command`, `shift`, `alt`; camera feel as
    /// `orbit_sensitivity`, `zoom_speed`, `invert_yaw`, `invert_pitch`,
    /// `invert_zoom`.
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()
            .map_err(|error| format!("invalid interaction profiles: {error}"))?;
        serde_json::to_string(self)
            .map_err(|error| format!("invalid interaction profile JSON: {error}"))
    }

    /// Parse and validate a document, rejecting unknown fields. The version is
    /// probed first, so a future version is reported before any profile body
    /// is parsed.
    pub fn from_json(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct VersionProbe {
            version: u32,
        }

        let probe: VersionProbe = serde_json::from_str(text)
            .map_err(|error| format!("invalid interaction profile JSON: {error}"))?;
        if probe.version != INTERACTION_FORMAT_VERSION {
            return Err(format!(
                "invalid interaction profiles: interaction profile version {} is not supported; \
                 expected version {INTERACTION_FORMAT_VERSION}",
                probe.version
            ));
        }

        let document: Self = serde_json::from_str(text)
            .map_err(|error| format!("invalid interaction profile JSON: {error}"))?;
        document
            .validate()
            .map_err(|error| format!("invalid interaction profiles: {error}"))?;
        Ok(document)
    }
}

/// Validated user interaction profiles beside the built-in `Default`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileLibrary {
    user: Vec<InteractionProfile>,
}

impl ProfileLibrary {
    /// Build a library from user profiles, validating each record, rejecting
    /// the built-in name, duplicate names (case-sensitively) and more than
    /// [`MAX_USER_PROFILES`] entries.
    pub fn new(user: Vec<InteractionProfile>) -> Result<Self, String> {
        validate_profiles(&user)?;
        if user.len() > MAX_USER_PROFILES {
            return Err(cap_error(user.len()));
        }
        Ok(Self { user })
    }

    /// The user profiles, in library order.
    pub fn user(&self) -> &[InteractionProfile] {
        &self.user
    }

    /// The number of user profiles.
    pub fn len(&self) -> usize {
        self.user.len()
    }

    /// Whether the library holds no user profiles.
    pub fn is_empty(&self) -> bool {
        self.user.is_empty()
    }

    /// Insert a validated profile, replacing a same-named entry in place
    /// (order preserved) or appending a new one. The built-in name and the cap
    /// are refused; a rejected upsert changes nothing.
    pub fn upsert(&mut self, profile: InteractionProfile) -> Result<(), String> {
        profile
            .validate()
            .map_err(|error| format!("invalid profile {:?}: {error}", profile.name))?;
        if profile.name == DEFAULT_PROFILE_NAME {
            return Err(format!(
                "{DEFAULT_PROFILE_NAME:?} is a built-in profile and cannot be overwritten"
            ));
        }
        if let Some(existing) = self
            .user
            .iter_mut()
            .find(|existing| existing.name == profile.name)
        {
            *existing = profile;
            return Ok(());
        }
        if self.user.len() >= MAX_USER_PROFILES {
            return Err(cap_error(self.user.len() + 1));
        }
        self.user.push(profile);
        Ok(())
    }

    /// Remove the user profile named `name`; the built-in is never removed.
    pub fn remove(&mut self, name: &str) -> bool {
        if name == DEFAULT_PROFILE_NAME {
            return false;
        }
        let Some(index) = self.user.iter().position(|profile| profile.name == name) else {
            return false;
        };
        self.user.remove(index);
        true
    }

    /// The built-in `Default` first, then the user profiles in library order.
    pub fn all(&self) -> Vec<InteractionProfile> {
        let mut all = Vec::with_capacity(self.user.len() + 1);
        all.push(InteractionProfile::default());
        all.extend(self.user.iter().cloned());
        all
    }

    /// Parse a [`ProfileDocument`] and merge every profile atomically, returning
    /// the user-profile count after the merge. Any error leaves the library
    /// unchanged.
    pub fn import_text(&mut self, text: &str) -> Result<usize, String> {
        let document = ProfileDocument::from_json(text)?;
        let mut candidate = self.clone();
        for profile in document.profiles {
            candidate.upsert(profile)?;
        }
        *self = candidate;
        Ok(self.user.len())
    }

    /// Serialize the user profiles as a compact [`ProfileDocument`] JSON text,
    /// or an empty string when the library has no user profiles. Built-ins are
    /// never exported.
    pub fn export_text(&self) -> String {
        if self.user.is_empty() {
            return String::new();
        }
        let document = ProfileDocument {
            version: INTERACTION_FORMAT_VERSION,
            profiles: self.user.clone(),
        };
        document.to_json().unwrap_or_else(|error| {
            log::warn!("user interaction profiles could not be exported: {error}");
            String::new()
        })
    }
}

fn validate_profiles(profiles: &[InteractionProfile]) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(profiles.len());
    for profile in profiles {
        profile
            .validate()
            .map_err(|error| format!("invalid profile {:?}: {error}", profile.name))?;
        if profile.name == DEFAULT_PROFILE_NAME {
            return Err(format!(
                "{DEFAULT_PROFILE_NAME:?} is a built-in profile and cannot be stored twice"
            ));
        }
        if !seen.insert(profile.name.as_str()) {
            return Err(format!(
                "duplicate profile name {:?}; profile names must be unique",
                profile.name
            ));
        }
    }
    Ok(())
}

fn cap_error(count: usize) -> String {
    format!("at most {MAX_USER_PROFILES} user profiles are supported; got {count}")
}
