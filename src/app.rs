use std::sync::Arc;
use std::time::Instant;

use eframe::egui;
use glam::{Vec2, Vec3};

use crate::camera::OrbitCamera;
use crate::render::Camera;
use crate::renderer::{DrawCallback, DrawState, LineInstanceGPU, SceneGPU};
use crate::screenshot::{self, Screenshot};
use crate::streamlines::click_seed;
use crate::persistence::{self, AppSettings, ViewRecord};
use crate::ui::formula_bar::FormulaBarState;
use crate::ui::layout::{self, LayoutState};
use crate::ui::menu::{self, MenuAction};
use crate::ui::presets::{PresetPanel, default_preset_dir};
use crate::ui::sidebar::Controls;
use crate::ui::style_panel::{self, AppStyle};
use crate::ui::top_bar;
use crate::view_command::{self, ViewCommand, ViewContext};
use crate::viewmodel::{
    ClickOutcome,
    DragButtons,
    DragChannel,
    LastClickSeed,
    State,
    ViewGesture,
    apply_camera_input,
    click_outcome,
    drag_channel,
    undo_click_seed,
};

// Particle cloud (ticket 05 / plan task 10): the viewmodel owns the toggle,
// the app owns the cloud and decides when to run these pure functions.
use crate::particles::{
    CLICK_BURST_PARTICLES,
    Particles,
    DOT_SPACING_FRACTION,
    advect,
    clear,
    dot_instances,
    spawn,
    spawn_at,
    target_count,
    count as particle_count,
};

/// Directory where the app persists camera presets. Kept in one place so the
/// panel and any export helper always agree (preset-persistence plan Task 3).
pub fn presets_data_dir() -> std::path::PathBuf {
    default_preset_dir()
}

/// What the status bar reports this frame: the latest formula parse state
/// (empty when the field is good) and the size of the scene being painted —
/// field geometry only (arrows + streamline segments + dots; the app's
/// backdrop lines are decoration and are excluded from the count).
pub struct StatusBarState {
    pub parse_message: String,
    pub instance_count: usize,
    pub streamline_count: usize,
}

/// How many viewport clicks keep their seeded bursts. Clicks accumulate only up
/// to this bound (the oldest burst is dropped), so repeated clicking cannot
/// grow the scene without limit.
const MAX_CLICK_SEEDS: usize = 6;

// Double-click seed handling (ticket 18): the first click seeds immediately and
// a second click within `viewmodel::DOUBLE_CLICK_WINDOW` takes that burst back
// before resetting the view. This is an accepted transient — a deferred seed
// would add ~300 ms of latency to every single click, while the final state
// after a double-click is clean. A triple-click is not special-cased: egui
// reports the third click as a plain click, so it seeds again after the reset.

/// Application root state: the viewmodel State (the single source of truth for
/// what is on screen), the formula bar's local widgets, the sidebar's local
/// Controls, the status readouts, both cameras (the 2D `Camera` and the
/// `OrbitCamera`), the preset manager, the persisted appearance and layout
/// records, and the GPU scene currently being painted.
pub struct VfApp {
    pub state: State,
    pub bar: FormulaBarState,
    pub sidebar: Controls,
    pub status: StatusBarState,
    /// The 2D camera record the preset panel saves/applies; the live view is
    /// the shared [`OrbitCamera`] (its ortho mode is the 2D view).
    pub camera: Camera,
    pub orbit: OrbitCamera,
    pub presets: PresetPanel,
    /// The persisted look-and-feel record (ticket 07): restored from eframe's
    /// app-state storage on startup and written back in [`eframe::App::save`].
    /// It lives app-side and never touches the viewmodel `State`.
    pub style: AppStyle,
    /// The persisted layout record (ticket 14): panel visibility, expanded
    /// widths and help-overlay visibility.
    pub layout: LayoutState,
    /// Bumped by "Reset layout" so the panels get fresh egui ids and therefore
    /// forget their last dragged size, letting the default widths apply.
    panel_generation: u32,
    /// Whether the Settings > Appearance… popover is up (ticket 15).
    settings_open: bool,
    /// Whether the Help > About dialog is up (ticket 15).
    about_open: bool,
    /// Whether the Help > Keyboard & mouse window is up (ticket 18): the full
    /// help table, shared with the viewport overlay.
    help_open: bool,
    /// The viewport rect from this frame (set in the central panel) so the help
    /// overlay can anchor to its bottom-left corner.
    last_viewport_rect: Option<egui::Rect>,
    scene: SceneGPU,
    renderer_ready: bool,
    streamline_count: usize,
    /// The advected cloud behind the "Particles" toggle (ticket 05): 3D-view
    /// only; its per-frame dots ramp with `dots_range`, the same magnitude
    /// range the arrows were coloured with at the last scene rebuild.
    particles: Particles,
    dots_range: (f32, f32),
    /// Where the user clicked in the viewport (ticket 06), bounded by
    /// [`MAX_CLICK_SEEDS`] and oldest-first. The viewmodel turns each pick into
    /// a deterministic streamline burst; the newest click also drops a
    /// particle burst when the particle toggle is on.
    click_seeds: Vec<Vec3>,
    /// The most recent single click's seed, so a double-click can take it back
    /// (ticket 18: double-click resets the view, it must not seed).
    last_click_seed: Option<LastClickSeed>,
    /// The grid-seeded streamline segments from the last scene rebuild, kept
    /// separately so a click can recompose the line collection without
    /// re-sampling or re-tracing the grid.
    flow_lines: Vec<LineInstanceGPU>,
    /// The current click bursts' streamline segments.
    click_lines: Vec<LineInstanceGPU>,
    /// The screenshot action (ticket 08): the pending capture request, the
    /// channel its save thread reports on, and the completion toast.
    pub screenshot: Screenshot,
}

impl VfApp {
    /// App with no persistent storage (defaults for every persisted record).
    pub fn new() -> Self {
        Self::new_with_storage(None)
    }

    /// App wired to eframe's persistent storage: the settings envelope
    /// (style + layout + view state) is restored from it — absent, corrupt or
    /// older records fall back to sensible defaults — and written back on save.
    pub fn new_with_storage(storage: Option<&dyn eframe::Storage>) -> Self {
        let settings = persistence::load(storage);
        let mut state = State::new(false);
        let mut sidebar = Controls::new();
        // Restore through the viewmodel setters so the first frame samples the
        // saved field; a corrupt record cannot poison the grid.
        settings.view.restore(&mut state, &mut sidebar);
        Self {
            state,
            bar: FormulaBarState::new(),
            sidebar,
            status: StatusBarState {
                parse_message: String::new(),
                instance_count: 0,
                streamline_count: 0,
            },
            camera: Camera::default(),
            orbit: OrbitCamera::default(),
            presets: PresetPanel::new_with_dir(presets_data_dir())
                .unwrap_or_else(|_| PresetPanel::new()),
            style: settings.style,
            layout: settings.layout,
            panel_generation: 0,
            settings_open: false,
            about_open: false,
            help_open: false,
            last_viewport_rect: None,
            scene: SceneGPU {
                arrows: Arc::new(Vec::new()),
                lines: Arc::new(reference_lines()),
                dots: Arc::new(Vec::new()),
            },
            renderer_ready: false,
            streamline_count: 0,
            particles: Particles::default(),
            dots_range: (0.0, 1.0),
            click_seeds: Vec::new(),
            last_click_seed: None,
            flow_lines: Vec::new(),
            click_lines: Vec::new(),
            screenshot: Screenshot::new(),
        }
    }

    /// Apply one menu or shortcut action through the pure seam, then run the
    /// side effect that needs the app (popover, capture, clipboard, quit).
    /// Everything the seam can mutate goes in through [`menu::UiState`], so
    /// there is no second copy of the layout, camera or controls records.
    fn apply_menu_action(&mut self, action: MenuAction, ctx: &egui::Context) {
        let effect = menu::apply_action(
            action,
            &mut menu::UiState {
                layout: &mut self.layout,
                orbit: &mut self.orbit,
                camera: &mut self.camera,
                state: &mut self.state,
                controls: &mut self.sidebar,
            },
        );
        match effect {
            menu::SideEffect::None => {}
            menu::SideEffect::OpenSettings => self.settings_open = true,
            menu::SideEffect::ShowAbout => self.about_open = true,
            menu::SideEffect::ShowHelp => self.help_open = true,
            // The capture itself happens in the viewport, where this frame's
            // rect and camera are known.
            menu::SideEffect::Screenshot => self.screenshot.request(),
            menu::SideEffect::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            menu::SideEffect::CopyText(text) => ctx.copy_text(text),
            menu::SideEffect::ResetLayout => {
                // New panel ids forget egui's remembered drag width, so the
                // default widths really apply.
                self.panel_generation = self.panel_generation.wrapping_add(1);
            }
            menu::SideEffect::ResetCamera => self.presets.clear_active(),
        }
    }

    /// The current view context for the command seam: which projection is
    /// showing, which domain it frames, and the viewport's aspect ratio. The
    /// aspect comes from the last painted viewport (keyboard and panel buttons
    /// run before the central panel), so a resize takes effect one frame later.
    fn view_context(&self) -> ViewContext {
        let aspect = self
            .last_viewport_rect
            .map(|rect| rect.width().max(1.0) / rect.height().max(1.0))
            .unwrap_or(1.0);
        ViewContext::new(self.state.mode_3d, self.state.domain, aspect)
    }

    /// Apply one camera command from any input surface (keyboard, double-click,
    /// camera buttons, menu). Reset and fit invalidate the highlighted preset:
    /// the camera no longer matches the saved view.
    fn apply_view_command(&mut self, command: ViewCommand) {
        self.orbit = view_command::apply_view_command(self.orbit, self.view_context(), command);
        if matches!(command, ViewCommand::Reset | ViewCommand::Fit) {
            self.presets.clear_active();
        }
    }

    /// Turn one viewport click into a deterministic streamline burst (and a
    /// particle burst when particles are on). The pure entry point maps the
    /// pointer position onto the camera ray (3D, at the focal plane) or the 2D
    /// plane; the viewmodel retraces the bounded burst list.
    fn seed_click(&mut self, pos: egui::Pos2, rect: egui::Rect, aspect: f32, time: f64) {
        let width = rect.width().max(1.0);
        let height = rect.height().max(1.0);
        let ndc = Vec2::new(
            (pos.x - rect.left()) / width * 2.0 - 1.0,
            1.0 - (pos.y - rect.top()) / height * 2.0,
        );
        let ortho = if self.state.mode_3d { None } else { Some(self.state.plane_size()) };
        let Some(seed) = click_seed(self.orbit, &self.state.domain, aspect, ndc, ortho) else {
            // A click that lands off the domain plants nothing, so it cannot
            // be the first half of a seeding double-click either.
            self.last_click_seed = None;
            return;
        };
        if self.click_seeds.len() >= MAX_CLICK_SEEDS {
            self.click_seeds.remove(0);
        }
        self.click_seeds.push(seed);
        self.click_lines = self.state.click_streamlines_snapshot(self.dots_range, &self.click_seeds);
        self.streamline_count = self.flow_lines.len() + self.click_lines.len();
        self.scene.lines = Arc::new(compose_lines(&self.flow_lines, &self.click_lines));
        log::trace!(
            "click seed {} of {} at {seed:?}: {} click streamline segments",
            self.click_seeds.len(),
            MAX_CLICK_SEEDS,
            self.click_lines.len()
        );
        // Particles only exist in the 3D view; a click adds a capped burst
        // around the pick (the per-frame dot rebuild picks it up next frame).
        // If the cloud has not spawned yet, fill it first so the click does not
        // leave a cloud of just this burst. The count after that ambient spawn
        // is what a double-click truncates back to.
        let mut particles_before = particle_count(&self.particles);
        if self.state.particles && self.state.mode_3d {
            if particle_count(&self.particles) == 0 {
                spawn(
                    &mut self.particles,
                    &self.state.domain,
                    target_count(self.state.density),
                    3,
                );
            }
            particles_before = particle_count(&self.particles);
            spawn_at(
                &mut self.particles,
                &self.state.domain,
                seed,
                CLICK_BURST_PARTICLES,
                3,
            );
        }
        self.last_click_seed = Some(LastClickSeed { seed, particles_before, time });
    }

    /// Take back the burst the previous single click planted when it turns out
    /// to be the first half of a double-click: the double-click resets the view
    /// and must not leave a seed behind. The decision and the seed-list edit
    /// live in the pure [`undo_click_seed`] seam; this wrapper applies the
    /// particle truncation and recomposes the line collection.
    fn undo_last_click_seed(&mut self, now: f64) {
        let undo = undo_click_seed(
            &self.click_seeds,
            self.last_click_seed.take(),
            now,
            particle_count(&self.particles),
        );
        if !undo.removed {
            return;
        }
        self.click_seeds = undo.seeds;
        if self.state.particles
            && self.state.mode_3d
            && let Some(before) = undo.particles
        {
            self.particles.pos.truncate(before);
        }
        self.click_lines = self.state.click_streamlines_snapshot(self.dots_range, &self.click_seeds);
        self.streamline_count = self.flow_lines.len() + self.click_lines.len();
        self.scene.lines = Arc::new(compose_lines(&self.flow_lines, &self.click_lines));
    }
}

impl Default for VfApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Grid + axes backdrop on the z = 0 plane (thin unlit lines). App-side
/// decoration: the viewmodel snapshot only owns the sampled field geometry.
fn reference_lines() -> Vec<LineInstanceGPU> {
    let mut lines = Vec::new();
    let half = 6f32;
    for i in -10..=10 {
        let t = i as f32 / 10.0 * half;
        let gray = if i == 0 { 0.45 } else { 0.22 };
        let axis_color = if i == 0 { [0.55, 0.2, 0.2, 1.0] } else { [gray, gray, gray + 0.03, 1.0] };
        lines.push(LineInstanceGPU {
            a: [t, -half, 0.0, 1.0],
            b: [t, half, 0.005, 0.015],
            color: axis_color,
        });
    }
    for i in -10..=10 {
        let t = i as f32 / 10.0 * half;
        let gray = if i == 0 { 0.45 } else { 0.22 };
        let axis_color = if i == 0 { [0.2, 0.55, 0.2, 1.0] } else { [gray, gray + 0.03, gray, 1.0] };
        lines.push(LineInstanceGPU {
            a: [-half, t, 0.0, 1.0],
            b: [half, t, 0.005, 0.015],
            color: axis_color,
        });
    }
    lines
}

/// A slim collapse chevron at the top of an expanded panel. Clicking it
/// collapses the panel back to its rail — the symmetric affordance to the
/// rail's expand click, alongside Ctrl+B / Ctrl+P.
fn collapse_button(ui: &mut egui::Ui, side: layout::RailSide, shortcut: &str) -> bool {
    let (chevron, tooltip) = match side {
        layout::RailSide::Left => ("‹", format!("Collapse Controls ({shortcut})")),
        layout::RailSide::Right => ("›", format!("Collapse Presets ({shortcut})")),
    };
    let mut clicked = false;
    ui.horizontal(|ui| {
        clicked = ui.small_button(chevron).on_hover_text(tooltip).clicked();
    });
    clicked
}

/// Fold the backdrop, the grid-seeded streamlines and the click bursts into the
/// one line collection the viewport paints. Rebuilt only on a scene rebuild or
/// a click, so the per-frame path still clones an `Arc`.
fn compose_lines(flow: &[LineInstanceGPU], clicks: &[LineInstanceGPU]) -> Vec<LineInstanceGPU> {
    let mut lines = reference_lines();
    lines.extend_from_slice(flow);
    lines.extend_from_slice(clicks);
    lines
}

impl eframe::App for VfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Restyle the whole frame from the persisted record before any widget
        // runs. `apply` is idempotent, so doing this every frame is cheap and
        // keeps the UI in sync even if a theme/visual was changed elsewhere.
        style_panel::apply(ui.ctx(), &self.style);

        // Screenshot completions arrive on a channel from the save thread; pick
        // them up here so the toast reflects the latest attempt this frame.
        self.screenshot.tick(Instant::now());

        // Menu shortcuts (ticket 15): one table in `ui::menu`, consumed before
        // any widget so a focused text field never receives a key the menu
        // owns. Ctrl+Q is the one global shortcut (it quits from anywhere);
        // view state is already persisted, so there is no save prompt.
        let ctx = ui.ctx().clone();
        for action in menu::shortcut_actions(&ctx) {
            self.apply_menu_action(action, &ctx);
        }

        // Camera keys (ticket 18): arrows, `+`/`-`, `R` and `F` feed the same
        // view-command seam as the double-click and the camera buttons. The
        // loop is inert while a text field has focus, so the formula bar keeps
        // every key it needs (the menu's global shortcuts already ran above).
        for command in view_command::view_key_commands(&ctx, &self.orbit, self.view_context()) {
            self.apply_view_command(command);
        }

        // `?` is the second help key (F1 goes through the menu table); Esc
        // dismisses the transient help surfaces. Neither fires while typing.
        if menu::help_key_pressed(&ctx) {
            self.layout.toggle_help();
        }
        if !ctx.text_edit_focused() && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
            self.help_open = false;
            self.layout.help_visible = false;
        }

        // Top menu bar: File / Edit / View / Settings / Help and the screenshot
        // action. The bar only reports the action; the app dispatches it.
        let mut style_changed = false;
        egui::Panel::top("top_bar").show(ui, |ui| {
            let (action, changed) = top_bar::show(
                ui,
                &mut self.style,
                &self.screenshot,
                &mut self.settings_open,
            );
            style_changed = changed;
            if let Some(action) = action {
                self.apply_menu_action(action, &ctx);
            }
        });
        if style_changed {
            style_panel::apply(ui.ctx(), &self.style);
        }

        // Formula bar below the menu row: live formula edits and the 2D/3D
        // toggle. Field presets load from the grouped Presets panel. Every
        // edit goes through a viewmodel setter, which owns parsing, clamping
        // and the dirty flag.
        egui::Panel::top("formula_bar").show(ui, |ui| {
            crate::ui::formula_bar::show(ui, &mut self.state, &mut self.bar);
        });

        // Left Controls panel, or its collapsed rail. The panel is shown
        // before the scene rebuild below so a control edit is reflected in this
        // frame's snapshot and status-bar count. Its width is remembered in the
        // layout record and restored on the next launch.
        if self.layout.controls_expanded {
            let mut collapse = false;
            let response = egui::Panel::left(("controls_panel", self.panel_generation))
                .resizable(true)
                .default_size(self.layout.controls_width)
                .min_size(layout::MIN_PANEL_WIDTH)
                .max_size(layout::MAX_PANEL_WIDTH)
                .show(ui, |ui| {
                    collapse = collapse_button(ui, layout::RailSide::Left, menu::CONTROLS_SHORTCUT);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        crate::ui::sidebar::show(ui, &mut self.state, &mut self.sidebar);
                    });
                });
            self.layout.controls_width = layout::clamp_width(
                response.response.rect.width(),
                layout::DEFAULT_CONTROLS_WIDTH,
            );
            if collapse {
                self.layout.controls_expanded = false;
            }
        } else {
            egui::Panel::left(("controls_rail", self.panel_generation))
                .exact_size(layout::RAIL_WIDTH)
                .show(ui, |ui| {
                    if layout::rail(
                        ui,
                        layout::RailSide::Left,
                        "Controls",
                        menu::CONTROLS_SHORTCUT,
                    ) {
                        self.layout.controls_expanded = true;
                    }
                });
        }

        // Right Presets panel, or its rail; same pin/collapse contract. The
        // preset record's camera mirrors the live 2D view (target + zoom), so
        // "Save current view" captures what is on screen; loading a preset
        // hands its camera back and moves the orbit target/zoom.
        self.camera.position = [self.orbit.target.x, self.orbit.target.y];
        self.camera.scale = self.orbit.ortho_zoom;
        // A camera button (Reset view / Fit domain) picked in the Presets panel
        // this frame; applied after the panel so the view-command seam owns the
        // camera update.
        let mut camera_request = None;
        if self.layout.presets_expanded {
            let mut collapse = false;
            let response = egui::Panel::right(("presets_panel", self.panel_generation))
                .resizable(true)
                .default_size(self.layout.presets_width)
                .min_size(layout::MIN_PANEL_WIDTH)
                .max_size(layout::MAX_PANEL_WIDTH)
                .show(ui, |ui| {
                    collapse = collapse_button(ui, layout::RailSide::Right, menu::PRESETS_SHORTCUT);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        camera_request =
                            self.presets.show(ui, &mut self.camera, &mut self.state);
                    });
                });
            self.layout.presets_width = layout::clamp_width(
                response.response.rect.width(),
                layout::DEFAULT_PRESETS_WIDTH,
            );
            if collapse {
                self.layout.presets_expanded = false;
            }
        } else {
            egui::Panel::right(("presets_rail", self.panel_generation))
                .exact_size(layout::RAIL_WIDTH)
                .show(ui, |ui| {
                    if layout::rail(
                        ui,
                        layout::RailSide::Right,
                        "Presets",
                        menu::PRESETS_SHORTCUT,
                    ) {
                        self.layout.presets_expanded = true;
                    }
                });
        }
        if let Some(command) = camera_request {
            self.apply_view_command(command);
        }
        if let Some(applied) = self.presets.take_applied() {
            // Guard the hand-editable preset file: `f32::clamp` passes NaN
            // through, so a non-finite component keeps the current camera
            // (review M-3).
            if applied.position[0].is_finite() {
                self.orbit.target.x = applied.position[0];
            }
            if applied.position[1].is_finite() {
                self.orbit.target.y = applied.position[1];
            }
            if applied.scale.is_finite() {
                self.orbit.ortho_zoom = applied
                    .scale
                    .clamp(OrbitCamera::MIN_ORTHO_ZOOM, OrbitCamera::MAX_ORTHO_ZOOM);
            }
        }

        // Rebuild the scene only when a setter marked the state dirty. The
        // app keeps owning the backdrop lines: fresh snapshots arrive on top
        // of a rebuilt grid + axes, and the streamline segments the viewmodel
        // seam emitted join them.
        if let Some(fresh) = self.state.snapshot_if_dirty() {
            self.scene.arrows = fresh.arrows;
            self.scene.dots = fresh.dots;
            // A rebuild refreshes the magnitude range the per-frame dots ramp
            // with (one extra sample pass on rebuild frames only), so dots
            // keep matching the arrows' colours.
            self.dots_range = self.state.color_range(&self.state.sample());
            self.flow_lines = Arc::unwrap_or_clone(fresh.lines);
            // Click bursts outlive the rebuild: they are anchored on the field,
            // not on the sampling grid, so retrace them against the new
            // domain/formula rather than dropping the user's picks.
            self.click_lines = self.state.click_streamlines_snapshot(self.dots_range, &self.click_seeds);
            self.streamline_count = self.flow_lines.len() + self.click_lines.len();
            self.scene.lines = Arc::new(compose_lines(&self.flow_lines, &self.click_lines));
            log::trace!(
                "scene rebuilt: {} arrows, {} streamline segments, {} dots",
                self.scene.arrows.len(),
                self.streamline_count,
                self.scene.dots.len()
            );
        }

        // Particle cloud: while the toggle is on (3D view only — the 2D
        // camera has no depth for the dots to sit in), keep the cloud alive,
        // advect it by the frame's stable dt and rebuild the dot instances so
        // they ride the same color_range as the arrows. Placed after the
        // dirty-rebuild block so this frame's dots already see a fresh
        // density/speed, and before the status bar so its count includes them.
        if self.state.particles && self.state.mode_3d {
            if particle_count(&self.particles) == 0 {
                spawn(&mut self.particles, &self.state.domain, target_count(self.state.density), 3);
            }
            let dt = ui.ctx().input(|i| i.stable_dt).max(0.0);
            advect(&mut self.particles, &self.state.field, &self.state.domain, dt, self.state.speed);
            self.scene.dots = Arc::new(
                dot_instances(
                    &self.particles,
                    &self.state.field,
                    &self.state.domain,
                    self.state.colormap,
                    self.dots_range,
                    DOT_SPACING_FRACTION * self.state.spacing(),
                ),
            );
            log::trace!(
                "particles: {} advected, {} dot instances",
                particle_count(&self.particles),
                self.scene.dots.len()
            );
        } else if particle_count(&self.particles) > 0 {
            // Toggle off (or the 2D view): drop the cloud so switching back
            // spawns fresh at the fixed seed; the dots were already emptied
            // because the toggle marks the state dirty.
            clear(&mut self.particles);
        }

        // Bottom status bar: the parse state (red when bad) plus how many
        // field-geometry instances are being painted — arrows, streamline
        // segments and dots; the app's backdrop lines are decoration and
        // stay out of the count, while the streamline portion gets its own
        // readout so toggling streamlines moves a visible number.
        self.status.parse_message = self.state.parse_error.clone();
        self.status.instance_count =
            self.scene.arrows.len() + self.streamline_count + self.scene.dots.len();
        self.status.streamline_count = self.streamline_count;
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            crate::ui::status_bar::show(ui, &self.status);
        });

        // Central viewport: the wgpu field renderer, driven by the same state.
        egui::CentralPanel::default_margins().show(ui, |ui| {
            let rect = ui.max_rect().shrink(2.0);
            self.last_viewport_rect = Some(rect);
            let Some(render_state) = frame.wgpu_render_state() else {
                ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(30, 33, 41));
                return;
            };

            if !self.renderer_ready {
                let mut renderer = render_state.renderer.write();
                renderer
                    .callback_resources
                    .insert(DrawState::new(&render_state.device, render_state.target_format));
                self.renderer_ready = true;
                log::info!("wgpu renderer initialized");
            }

            // Camera gestures feed the seam's pure camera action. Ticket 18
            // maps the buttons through `drag_channel`: in 3D left- and
            // right-drag orbit, middle-drag and Shift+right-drag pan; in 2D
            // every drag pans. The wheel/pinch factor zooms both views (the
            // camera clamps it).
            let resp = ui.allocate_response(rect.size(), egui::Sense::click_and_drag());
            let pointer_delta = resp.drag_delta();
            let pointer_delta = Vec2::new(pointer_delta.x, pointer_delta.y);
            let buttons = DragButtons {
                primary: resp.dragged_by(egui::PointerButton::Primary),
                secondary: resp.dragged_by(egui::PointerButton::Secondary),
                middle: resp.dragged_by(egui::PointerButton::Middle),
                shift: ui.ctx().input(|input| input.modifiers.shift),
            };
            let channel = drag_channel(self.state.mode_3d, buttons);
            let (drag, pan) = match channel {
                Some(DragChannel::Orbit) => (pointer_delta, Vec2::ZERO),
                Some(DragChannel::Pan) => (Vec2::ZERO, pointer_delta),
                None => (Vec2::ZERO, Vec2::ZERO),
            };
            let zoom = if resp.hovered() || resp.dragged() {
                crate::ui::top_bar::wheel_zoom(ui.ctx())
            } else {
                1.0
            };
            self.orbit = apply_camera_input(
                self.orbit,
                self.state.mode_3d,
                self.state.plane_size(),
                ViewGesture {
                    drag,
                    pan,
                    zoom,
                    viewport_height: rect.height(),
                },
            );

            // Cursor feedback (ticket 18): orbiting shows grab/grabbing,
            // panning shows move; the wheel zoom leaves the cursor alone.
            if resp.hovered() || resp.dragged() {
                let icon = match channel {
                    Some(DragChannel::Orbit) => egui::CursorIcon::Grabbing,
                    Some(DragChannel::Pan) => egui::CursorIcon::Move,
                    None if self.state.mode_3d => egui::CursorIcon::Grab,
                    None => egui::CursorIcon::Move,
                };
                ui.ctx().set_cursor_icon(icon);
            }

            let aspect = rect.width().max(1.0) / rect.height().max(1.0);

            // Click-to-seed and double-click reset (ticket 18): a click only
            // seeds when the gesture never became a drag, and a double-click
            // resets the view instead — taking back the seed its first click
            // planted so the gesture leaves no burst behind.
            let now = ui.ctx().input(|input| input.time);
            match click_outcome(resp.clicked(), resp.double_clicked(), resp.dragged()) {
                ClickOutcome::ResetView => {
                    self.undo_last_click_seed(now);
                    self.apply_view_command(ViewCommand::Reset);
                }
                ClickOutcome::Seed => {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        self.seed_click(pos, rect, aspect, now);
                    }
                }
                ClickOutcome::None => {}
            }

            // Painter's algorithm: the wgpu pass has no depth attachment, so
            // order the instances far-to-near along this frame's view.
            let view = self.orbit.view();
            let mut arrows = Arc::unwrap_or_clone(Arc::clone(&self.scene.arrows));
            crate::renderer::sort_back_to_front(arrows.as_mut_slice(), &view);
            self.scene.arrows = Arc::new(arrows);

            let scene = SceneGPU {
                arrows: Arc::clone(&self.scene.arrows),
                lines: Arc::clone(&self.scene.lines),
                dots: Arc::clone(&self.scene.dots),
            };

            // 2D mode covers the domain's plane orthographically; 3D mode is
            // the orbiting perspective camera.
            let view_proj = self.orbit.vp_matrix(
                aspect,
                if self.state.mode_3d { None } else { Some(self.state.plane_size()) },
            );

            // Screenshot export (ticket 08): re-render this frame's scene
            // offscreen at the viewport's pixel size (capped at 4096 on the
            // long edge, both axes scaled so the framing is unchanged) and
            // hand the pixels to a worker thread that shows the native save
            // dialog and writes the PNG. Nothing here touches the live
            // surface textures, and the readback is RGBA by contract.
            if self.screenshot.take_request() {
                // `rect` is in logical points; a capture must have the viewport's
                // *physical* pixels, so scale by the display's pixels-per-point
                // before rounding (HiDPI fix round 1). The cap applies after that.
                let pixels_per_point = ui.ctx().pixels_per_point();
                let (capture_width, capture_height) = screenshot::capture_size(
                    rect.width(),
                    rect.height(),
                    pixels_per_point,
                    screenshot::MAX_CAPTURE_EDGE,
                );
                // egui paints this behind the field; clearing to the same
                // (linear-space) fill keeps the capture's background identical.
                let clear = egui::Rgba::from(ui.visuals().panel_fill).to_array();
                let capture = {
                    let mut renderer = render_state.renderer.write();
                    renderer
                        .callback_resources
                        .get_mut::<DrawState>()
                        .expect("DrawState must be installed before capturing")
                        .render_offscreen(
                            &render_state.device,
                            &render_state.queue,
                            &scene,
                            view_proj,
                            self.orbit.eye_position(!self.state.mode_3d),
                            capture_width,
                            capture_height,
                            [
                                f64::from(clear[0]),
                                f64::from(clear[1]),
                                f64::from(clear[2]),
                                f64::from(clear[3]),
                            ],
                        )
                };
                match capture {
                    Ok(rgba) => screenshot::spawn_save(
                        self.screenshot.sender(),
                        ui.ctx().clone(),
                        rgba,
                        capture_width,
                        capture_height,
                        screenshot::default_filename(
                            std::time::SystemTime::now(),
                            if self.state.mode_3d { "3D" } else { "2D" },
                        ),
                    ),
                    Err(error) => self.screenshot.fail(error),
                }
            }

            let callback = eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                DrawCallback {
                    scene,
                    view_proj,
                    camera_pos: self.orbit.eye_position(!self.state.mode_3d),
                },
            );
            ui.painter().add(callback);
        });

        // Help overlay last, so it paints over the viewport (and the field
        // callback). Anchored at the viewport's bottom-left corner; the ×
        // persists the hidden state through the layout record.
        if self.layout.help_visible
            && let Some(rect) = self.last_viewport_rect
        {
            let anchor = egui::pos2(rect.left() + 12.0, rect.bottom() - 12.0);
            if crate::ui::help_overlay::show(ui.ctx(), anchor, self.state.mode_3d) {
                self.layout.help_visible = false;
            }
        }

        // Help > About (ticket 15): a small window over everything, closed by
        // its × (its open flag is the window's visibility).
        top_bar::about_window(ui.ctx(), &mut self.about_open);

        // Help > Keyboard & mouse (ticket 18): the full help table, the same
        // strings the overlay above renders.
        crate::ui::help_overlay::help_window(ui.ctx(), &mut self.help_open);
    }

    /// Persist the settings envelope (style + layout + view state) under
    /// eframe's app key. Camera state is intentionally absent: presets are the
    /// way to restore a viewpoint.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let settings = AppSettings {
            style: self.style.clone(),
            layout: self.layout.clone(),
            view: ViewRecord::capture(&self.state),
        };
        persistence::save(storage, &settings);
    }
}
