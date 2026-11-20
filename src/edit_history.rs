//! Bounded, in-memory edit history (issue #86 slice A1).
//!
//! A history stores snapshots of the editable document, not runtime state. Each
//! gesture opens a transaction with [`EditHistory::begin`], records at most one
//! entry on [`EditHistory::commit`], and moves a single cursor through the undo
//! and redo stacks on [`EditHistory::undo`] / [`EditHistory::redo`]. Nothing
//! here touches the app, the renderer, persistence or input; slice A2 wires it
//! in. See `docs/superpowers/specs/2026-11-18-edit-history-contract.md`.

use crate::appearance_profile::AppearanceProfile;
use crate::parameters::ParameterSet;
use crate::persistence::ViewRecord;
use crate::scene_document::SceneCamera;

/// Recording past this many entries evicts the oldest one.
pub const MAX_HISTORY: usize = 64;

/// One explicit reset scope. Only runtime state (`Simulation`) is outside the
/// document snapshot, so only that scope is undo-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditScope {
    /// Colour mapping: palette, ramp, scale mode, quantity and range.
    Color,
    /// The orbit/2D camera pose.
    Camera,
    /// Runtime state: simulation time, particle trails/dots, click bursts.
    Simulation,
    /// The whole document: view fields plus appearance.
    All,
}

impl EditScope {
    /// Every scope, in menu order.
    pub const ALL: [Self; 4] = [
        Self::Color,
        Self::Camera,
        Self::Simulation,
        Self::All,
    ];

    /// The user-facing name, matching the menu label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Color => "Color",
            Self::Camera => "Camera",
            Self::Simulation => "Simulation",
            Self::All => "All",
        }
    }

    /// Whether resetting this scope is recorded as one history entry.
    ///
    /// Runtime state is not part of the document snapshot, so the simulation
    /// reset is an explicit undo-free action.
    pub fn records_history(self) -> bool {
        !matches!(self, Self::Simulation)
    }
}

/// The immutable document value stored in a history entry.
///
/// The appearance name is a sentinel: the snapshot carries the profile's style
/// data, not a user-facing name to restore.
#[derive(Debug, Clone, PartialEq)]
pub struct EditSnapshot {
    /// Formula, mode, plot settings, domain, density, arrows and colour.
    pub view: ViewRecord,
    /// Named parameter values.
    pub parameters: ParameterSet,
    /// Orbit/2D camera pose.
    pub camera: SceneCamera,
    /// Viewport style, layer styles and colour mapping.
    pub appearance: AppearanceProfile,
}

impl EditSnapshot {
    /// Build a snapshot from the four document parts.
    pub fn new(
        view: ViewRecord,
        parameters: ParameterSet,
        camera: SceneCamera,
        appearance: AppearanceProfile,
    ) -> Self {
        Self {
            view,
            parameters,
            camera,
            appearance,
        }
    }
}

/// A bounded undo/redo history with one open transaction at most.
///
/// The undo stack is capped at [`MAX_HISTORY`]; recording past the cap evicts
/// the oldest entry. The redo stack never outgrows the undo stack.
#[derive(Debug, Clone, PartialEq)]
pub struct EditHistory<S> {
    /// Snapshots before each recorded edit, oldest first.
    past: Vec<S>,
    /// Snapshots returned by undo, most recently undone last.
    future: Vec<S>,
    /// The pre-edit snapshot of the open transaction, if any.
    pending: Option<S>,
}

impl<S: Clone + PartialEq> EditHistory<S> {
    /// An empty history with no open transaction.
    pub fn new() -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            pending: None,
        }
    }

    /// The number of undo steps available.
    pub fn len(&self) -> usize {
        self.past.len()
    }

    /// The number of redo steps available.
    pub fn redo_len(&self) -> usize {
        self.future.len()
    }

    /// Whether there is nothing to undo.
    pub fn is_empty(&self) -> bool {
        self.past.is_empty()
    }

    /// Whether at least one undo step is available.
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Whether at least one redo step is available.
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Whether a transaction is open.
    pub fn is_recording(&self) -> bool {
        self.pending.is_some()
    }

    /// Capture the pre-edit snapshot once.
    ///
    /// A second `begin` during the same gesture keeps the first snapshot, so a
    /// gesture cannot nest accidentally.
    pub fn begin(&mut self, before: S) {
        if self.pending.is_none() {
            self.pending = Some(before);
        }
    }

    /// Record one entry if the document changed, then end the transaction.
    ///
    /// Returns `true` iff an entry was recorded. A commit without a `begin`
    /// records nothing. An unchanged document records nothing but still ends
    /// the transaction. A real change drops the redo tail and evicts the oldest
    /// entry at [`MAX_HISTORY`].
    pub fn commit(&mut self, after: &S) -> bool {
        let Some(before) = self.pending.take() else {
            return false;
        };
        if &before == after {
            return false;
        }
        self.push_past(before);
        self.future.clear();
        true
    }

    /// Discard the open transaction, leaving undo and redo untouched.
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// `begin` + `commit` in one call, with the same rules.
    ///
    /// When a transaction is already open, `record` still uses the original
    /// pending snapshot from [`begin`](Self::begin) as the pre-edit state; the
    /// `before` argument is ignored. A mid-transaction record therefore folds
    /// into the gesture's single undo step instead of opening a second one.
    pub fn record(&mut self, before: S, after: &S) -> bool {
        self.begin(before);
        self.commit(after)
    }

    /// Move back one entry: push `current` onto redo and return the previous
    /// snapshot. Returns `None` and changes nothing when there is nothing to
    /// undo.
    pub fn undo(&mut self, current: S) -> Option<S> {
        let previous = self.past.pop()?;
        self.future.push(current);
        Some(previous)
    }

    /// Move forward one entry: push `current` onto undo and return the next
    /// snapshot. Returns `None` and changes nothing when there is nothing to
    /// redo.
    pub fn redo(&mut self, current: S) -> Option<S> {
        let next = self.future.pop()?;
        self.push_past(current);
        Some(next)
    }

    /// Drop every entry, both stacks and any open transaction.
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
        self.pending = None;
    }

    /// Push onto the bounded undo stack, evicting the oldest entry when full.
    fn push_past(&mut self, snapshot: S) {
        if self.past.len() >= MAX_HISTORY {
            self.past.remove(0);
        }
        self.past.push(snapshot);
    }
}

impl<S: Clone + PartialEq> Default for EditHistory<S> {
    fn default() -> Self {
        Self::new()
    }
}
