//! Pure guided-onboarding-tour model (#99 slice A).
//!
//! The tour is a fixed, ordered table of stops plus a session-only progress
//! value. Each stop names a semantic anchor — a control, a panel, a viewport
//! region or the maximised viewport — never a pixel coordinate and never an
//! `egui` type. The UI slice resolves [`TourTarget`] through [`resolve`] and
//! looks the descriptor up in the live layout.
//!
//! Nothing here mutates the document, history, persistence or renderer. The
//! only mutable state is [`TourProgress`], and dropping it ends the tour with no
//! written side effect.

/// A side panel or chrome surface the tour can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    /// The left Display/Camera/Features/Coloring/Domain panel.
    Controls,
    /// The top formula, projection, adapter and transport bar.
    FormulaBar,
    /// The right preset catalogue and saved-scene panel.
    Presets,
    /// The right appearance and profile panel.
    Appearance,
    /// The right interaction and binding panel.
    Interaction,
    /// The left Sources scene editor.
    Sources,
    /// The left Induction scene editor.
    Induction,
    /// The left ODE workbench.
    Ode,
    /// The left Materials scene editor.
    Materials,
    /// The left Maxwell teaching panel.
    Maxwell,
    /// The top menu row.
    TopBar,
    /// The bottom status bar.
    StatusBar,
}

impl PanelId {
    /// Every panel surface, in the order the frame assembles them.
    pub const ALL: &'static [PanelId] = &[
        PanelId::TopBar,
        PanelId::FormulaBar,
        PanelId::Controls,
        PanelId::Sources,
        PanelId::Induction,
        PanelId::Ode,
        PanelId::Materials,
        PanelId::Maxwell,
        PanelId::Presets,
        PanelId::Appearance,
        PanelId::Interaction,
        PanelId::StatusBar,
    ];

    /// Stable semantic key, safe for comparisons in a future UI router.
    pub const fn key(self) -> &'static str {
        match self {
            PanelId::Controls => "controls",
            PanelId::FormulaBar => "formula_bar",
            PanelId::Presets => "presets",
            PanelId::Appearance => "appearance",
            PanelId::Interaction => "interaction",
            PanelId::Sources => "sources",
            PanelId::Induction => "induction",
            PanelId::Ode => "ode",
            PanelId::Materials => "materials",
            PanelId::Maxwell => "maxwell",
            PanelId::TopBar => "top_bar",
            PanelId::StatusBar => "status_bar",
        }
    }
}

/// A control the tour can point at. The key is the stable name a future UI
/// slice matches against its live widget ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlId {
    /// The preset search field.
    PresetSearch,
    /// The editable formula text.
    FormulaInput,
    /// The named parameters (`Variables`) editor.
    Parameters,
    /// The 2D projection toggle.
    View2D,
    /// The 3D projection toggle.
    View3D,
    /// The `Reset view` camera button.
    CameraReset,
    /// The `Fit domain` camera button.
    CameraFit,
    /// The active colormap selector.
    Colormap,
    /// The colour-quantity selector.
    ColorQuantity,
    /// The colour legend and range readout.
    ColorLegend,
    /// The streamline feature checkbox.
    Streamlines,
    /// The particle feature checkbox.
    Particles,
    /// The transport play/pause button. This control lives in the compact
    /// transport strip (chrome), not inside the formula/parameter panel; the
    /// tour points at [`PanelId::FormulaBar`] only because that strip is
    /// assembled as part of the same top bar.
    Play,
    /// The Sources `Add` button.
    SourceAdd,
    /// The Maxwell case selector.
    MaxwellCase,
    /// The Maxwell learning `Check residual` button.
    MaxwellResidualCheck,
    /// The scalar `Show heatmap` checkbox.
    ScalarHeatmap,
    /// The scalar contour-level slider.
    ScalarContours,
    /// The scalar `Place probe` checkbox.
    FieldProbePlace,
    /// The Field probes `Add sphere` button: a real integral surface probe.
    FieldProbeSurface,
    /// The appearance profile selector.
    ProfileSelector,
    /// The saved-scene `Save` button.
    SaveScene,
}

impl ControlId {
    /// Every control the tour model knows about.
    pub const ALL: &'static [ControlId] = &[
        ControlId::PresetSearch,
        ControlId::FormulaInput,
        ControlId::Parameters,
        ControlId::View2D,
        ControlId::View3D,
        ControlId::CameraReset,
        ControlId::CameraFit,
        ControlId::Colormap,
        ControlId::ColorQuantity,
        ControlId::ColorLegend,
        ControlId::Streamlines,
        ControlId::Particles,
        ControlId::Play,
        ControlId::SourceAdd,
        ControlId::MaxwellCase,
        ControlId::MaxwellResidualCheck,
        ControlId::ScalarHeatmap,
        ControlId::ScalarContours,
        ControlId::FieldProbePlace,
        ControlId::FieldProbeSurface,
        ControlId::ProfileSelector,
        ControlId::SaveScene,
    ];

    /// Stable semantic key. [`AdvanceRule::Changed`] stores these names.
    pub const fn key(self) -> &'static str {
        match self {
            ControlId::PresetSearch => "preset_search",
            ControlId::FormulaInput => "formula_input",
            ControlId::Parameters => "parameters",
            ControlId::View2D => "view_2d",
            ControlId::View3D => "view_3d",
            ControlId::CameraReset => "camera_reset",
            ControlId::CameraFit => "camera_fit",
            ControlId::Colormap => "colormap",
            ControlId::ColorQuantity => "color_quantity",
            ControlId::ColorLegend => "color_legend",
            ControlId::Streamlines => "streamlines",
            ControlId::Particles => "particles",
            ControlId::Play => "play",
            ControlId::SourceAdd => "source_add",
            ControlId::MaxwellCase => "maxwell_case",
            ControlId::MaxwellResidualCheck => "maxwell_residual_check",
            ControlId::ScalarHeatmap => "scalar_heatmap",
            ControlId::ScalarContours => "scalar_contours",
            ControlId::FieldProbePlace => "field_probe_place",
            ControlId::FieldProbeSurface => "field_probe_surface",
            ControlId::ProfileSelector => "profile_selector",
            ControlId::SaveScene => "save_scene",
        }
    }

    /// The panel that owns the control. The tour uses this to open a collapsed
    /// panel before highlighting the control.
    pub const fn panel(self) -> PanelId {
        match self {
            ControlId::PresetSearch
            | ControlId::CameraReset
            | ControlId::CameraFit
            | ControlId::SaveScene => PanelId::Presets,
            ControlId::FormulaInput | ControlId::View2D | ControlId::View3D | ControlId::Play => {
                // `Play` is chrome in the compact transport strip, not a panel
                // child; the top-bar panel is the closest anchor to open.
                PanelId::FormulaBar
            }
            ControlId::Parameters
            | ControlId::Colormap
            | ControlId::ColorQuantity
            | ControlId::ColorLegend
            | ControlId::Streamlines
            | ControlId::Particles
            | ControlId::ScalarHeatmap
            | ControlId::ScalarContours
            | ControlId::FieldProbePlace
            | ControlId::FieldProbeSurface => PanelId::Controls,
            ControlId::SourceAdd => PanelId::Sources,
            ControlId::MaxwellCase | ControlId::MaxwellResidualCheck => PanelId::Maxwell,
            ControlId::ProfileSelector => PanelId::Appearance,
        }
    }

    /// Whether the tour must anchor to the control's own painted rectangle
    /// rather than falling back to its containing panel. The integral-probe
    /// control needs this: an absent Field probes body must not highlight the
    /// whole Controls panel as if it were the surface probe.
    pub const fn requires_live_rect(self) -> bool {
        matches!(self, ControlId::FieldProbeSurface)
    }
}

/// A semantic sub-region of the central viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    /// The painted field area.
    Center,
    /// Top-left overlay corner.
    TopLeft,
    /// Top-right overlay corner.
    TopRight,
    /// Bottom-left overlay corner.
    BottomLeft,
    /// Bottom-right overlay corner.
    BottomRight,
}

impl Region {
    /// Every viewport region the tour can anchor to.
    pub const ALL: &'static [Region] = &[
        Region::Center,
        Region::TopLeft,
        Region::TopRight,
        Region::BottomLeft,
        Region::BottomRight,
    ];
}

/// What a stop points at. Coordinates never appear here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourTarget {
    /// One control inside a known panel.
    Control(ControlId),
    /// A whole panel, or its collapsed rail.
    Panel(PanelId),
    /// A semantic viewport region.
    Viewport(Region),
    /// The maximised viewport, with every panel collapsed.
    Maximize,
}

/// How the user leaves a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvanceRule {
    /// The explicit Next control (or Enter/Right).
    Next,
    /// Wait until the named [`ControlId`] reports a real change. The string is
    /// [`ControlId::key`], so a stop cannot wait on a capability the model does
    /// not know.
    Changed(&'static str),
}

/// One stop in the ordered tour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TourStop {
    /// Stable identifier, unique in [`TOUR`].
    pub id: &'static str,
    /// Card title.
    pub title: &'static str,
    /// Card body, describing only what the build already does.
    pub body: &'static str,
    /// Semantic anchor the UI highlights.
    pub target: TourTarget,
    /// How the user leaves the stop.
    pub advance: AdvanceRule,
}

/// A resolved anchor descriptor for the UI slice. No `egui` type appears here;
/// the UI maps the panel/control ids onto the live layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourAnchor {
    /// A control inside the panel that owns it.
    Control {
        /// Owning panel, to open first when it is collapsed.
        panel: PanelId,
        /// The control to highlight.
        control: ControlId,
    },
    /// A whole panel or its rail.
    Panel(PanelId),
    /// A viewport sub-region.
    Viewport(Region),
    /// The maximised viewport.
    Maximize,
}

/// Resolve a target into the descriptor a future UI slice looks up.
pub const fn resolve(target: TourTarget) -> TourAnchor {
    match target {
        TourTarget::Control(control) => TourAnchor::Control {
            panel: control.panel(),
            control,
        },
        TourTarget::Panel(panel) => TourAnchor::Panel(panel),
        TourTarget::Viewport(region) => TourAnchor::Viewport(region),
        TourTarget::Maximize => TourAnchor::Maximize,
    }
}

/// The ordered tour: welcome through done. Every target names a capability in
/// this build; a removed capability stops compiling in the model instead of
/// producing a runtime "feature missing" message.
pub const TOUR: &[TourStop] = &[
    TourStop {
        id: "welcome",
        title: "Welcome to Vector Field Explorer",
        body: "This short tour points at the parts of the workspace in the order \
               the lessons use them. Nothing here changes your scene, and you can \
               leave at any time.",
        target: TourTarget::Maximize,
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "presets",
        title: "Start from a preset",
        body: "The Presets panel groups built-in formula, physics, source and \
               gallery scenes. Loading one keeps the existing loader and its \
               camera framing.",
        target: TourTarget::Panel(PanelId::Presets),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "formula",
        title: "Formula and parameters",
        body: "The formula bar edits the field expression and switches between \
               2D and 3D. Named parameters live in the Variables editor in the \
               Controls panel, so one formula can be retuned without retyping it.",
        target: TourTarget::Control(ControlId::FormulaInput),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "viewport",
        title: "The vector viewport",
        body: "Drag to orbit, scroll to zoom and pan with the usual camera \
               gestures. Reset view and Fit domain restore a useful framing; \
               press Fit domain when you are ready to continue.",
        target: TourTarget::Viewport(Region::Center),
        advance: AdvanceRule::Changed("camera_fit"),
    },
    TourStop {
        id: "coloring",
        title: "Colouring and legend",
        body: "The Coloring section chooses the colormap, what the ramp represents \
               and how values map onto it. The legend below the controls reports \
               the effective range and any clipped samples.",
        target: TourTarget::Control(ControlId::Colormap),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "streamlines",
        title: "Streamlines and particles",
        body: "The Features section can draw streamlines from grid or click seeds \
               and animate particles through the same formula field. Turn on \
               Particles, then press Play in the transport to see motion.",
        target: TourTarget::Control(ControlId::Streamlines),
        advance: AdvanceRule::Changed("play"),
    },
    TourStop {
        id: "sources",
        title: "Build a source scene",
        body: "Switch the adapter to Sources to place charges, wires and other \
               bounded sources, then view the electric field, the magnetic field \
               or both from the same catalogue.",
        target: TourTarget::Panel(PanelId::Sources),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "maxwell",
        title: "Maxwell teaching case",
        body: "The Maxwell adapter offers canonical ideal geometries such as \
               parallel plates, coaxial cases, a solenoid and a bounded 2D \
               conductor. Choose a case in the Maxwell panel; its display values \
               stay in normalized teaching units.",
        target: TourTarget::Control(ControlId::MaxwellCase),
        advance: AdvanceRule::Changed("maxwell_case"),
    },
    TourStop {
        id: "maxwell_residual",
        title: "Check a Maxwell residual",
        body: "Open Maxwell equations in the same panel, choose an equation and a \
               probe point, then press Check residual. The check evaluates one \
               point only and reports the terms and residual honestly.",
        target: TourTarget::Control(ControlId::MaxwellResidualCheck),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "scalar",
        title: "Scalar views",
        body: "The Scalar view section paints a heatmap and marching-squares \
               contours from a scalar expression. Grid resolution, contour levels \
               and the legend all describe the slice you are looking at.",
        target: TourTarget::Control(ControlId::ScalarHeatmap),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "probes",
        title: "Field probes",
        body: "The Field probes section adds bounded Gaussian surfaces and \
               circulation loops. It reports flux and circulation estimates with \
               an explicit resolved, unresolved or singular status. This step \
               switches to the Sources adapter if the current adapter cannot \
               supply a probe field.",
        target: TourTarget::Control(ControlId::FieldProbeSurface),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "appearance",
        title: "Appearance and profiles",
        body: "The Appearance panel styles live layers and physics families, and \
               applies saved appearance profiles. These styles affect the live \
               and raster view only; SVG exports keep the blueprint.",
        target: TourTarget::Panel(PanelId::Appearance),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "saved_scenes",
        title: "Save a scene",
        body: "The Presets panel saves the current view and whole scenes, then \
               reloads them atomically through the existing scene store. Saving \
               is the one step that writes, and only when you choose it.",
        target: TourTarget::Control(ControlId::SaveScene),
        advance: AdvanceRule::Next,
    },
    TourStop {
        id: "done",
        title: "Tour complete",
        body: "You have seen the workspace in the order the lessons build on it. \
               Restart the tour when you are ready for this refresher again.",
        target: TourTarget::Maximize,
        advance: AdvanceRule::Next,
    },
];

/// Session-only tour progress. Kept outside the view record, history and every
/// persisted settings shape; dropping it ends the tour without writing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TourProgress {
    index: usize,
    opened_panels: Vec<PanelId>,
}

impl TourProgress {
    /// Start at the first stop with no panels opened by the tour.
    pub const fn new() -> Self {
        Self {
            index: 0,
            opened_panels: Vec::new(),
        }
    }

    /// Index of the current stop, or [`TOUR`]'s length once finished.
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Whether the tour has reached the end (finished or dismissed).
    pub const fn is_finished(&self) -> bool {
        self.index >= TOUR.len()
    }

    /// The current stop, or `None` after the last stop.
    pub fn current(&self) -> Option<&'static TourStop> {
        TOUR.get(self.index)
    }

    /// Move one stop forward, clamping at the end.
    pub fn advance(&mut self) {
        if !self.is_finished() {
            self.index += 1;
        }
    }

    /// Move one stop back, clamping at the first stop. This is a no-op once the
    /// tour is finished: [`dismiss`](Self::dismiss) and reaching the last stop
    /// both end the tour for the session, and going back must not reopen it.
    pub fn back(&mut self) {
        if self.is_finished() {
            return;
        }
        self.index = self.index.saturating_sub(1);
    }

    /// End the tour for the session without persisting anything. After this,
    /// [`back`](Self::back) is a no-op; only [`restart`](Self::restart) returns
    /// to the first stop.
    pub fn dismiss(&mut self) {
        self.index = TOUR.len();
    }

    /// Return to the first stop and forget which panels the tour opened.
    pub fn restart(&mut self) {
        self.index = 0;
        self.opened_panels.clear();
    }

    /// Record that the tour opened `panel` for a stop. Returns `true` only the
    /// first time, so a later stop does not close a panel another stop needed.
    pub fn mark_opened(&mut self, panel: PanelId) -> bool {
        if self.opened_panels.contains(&panel) {
            return false;
        }
        self.opened_panels.push(panel);
        true
    }

    /// Panels the tour has opened this run, in first-opened order.
    pub fn opened_panels(&self) -> &[PanelId] {
        &self.opened_panels
    }
}
