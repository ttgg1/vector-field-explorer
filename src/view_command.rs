//! The pure view-command seam (ticket 18): one [`ViewCommand`] type for every
//! camera action the app offers, applied by [`apply_view_command`].
//!
//! The keyboard, the viewport's double-click, the camera buttons in the
//! Presets panel and the View menu all build a command and hand it to this
//! function, so reset/fit/orbit/pan/zoom behave identically whatever the input
//! surface. Mouse dragging keeps its own gesture seam ([`crate::viewmodel`])
//! because a drag is a per-frame delta, not a one-shot command; both end in the
//! same [`OrbitCamera`] methods and clamps.

use eframe::egui;
use glam::Vec3;

use crate::camera::OrbitCamera;
use crate::domain::Domain;
use crate::fly_camera::FlyCamera;
use crate::interaction::{Bindings, CameraFeel, ChordModifiers, InteractionAction, KeyChord};
use crate::viewmodel::{LeftClickAction, PLANE_MARGIN};

/// Radians the view orbits per arrow-key press in 3D.
pub const KEY_ORBIT_STEP: f32 = 0.06;
/// A keyboard pan moves this fraction of the visible height per press.
pub const KEY_PAN_FRACTION: f32 = 0.05;
/// Shift+arrow in 2D pans this many times faster than a plain arrow.
pub const KEY_PAN_FAST_MULTIPLIER: f32 = 3.0;
/// Multiplicative zoom per `+`/`-` press (egui's convention: >1 gets closer).
pub const KEY_ZOOM_FACTOR: f32 = 1.2;

/// The view a command applies to: which projection is showing, which domain it
/// should frame, and the viewport's aspect ratio (width / height) for Fit. A
/// plain value so the seam stays pure and testable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewContext {
    pub mode_3d: bool,
    pub domain: Domain,
    /// Viewport width / height. Fit uses the tighter field of view; a
    /// non-finite or non-positive value falls back to 1.0.
    pub aspect: f32,
}

impl ViewContext {
    pub fn new(mode_3d: bool, domain: Domain, aspect: f32) -> Self {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        Self {
            mode_3d,
            domain,
            aspect,
        }
    }
}

/// One camera action, shared by every input surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewCommand {
    /// Rotate the orbit by `dx`/`dy` radians (2D ignores it).
    Orbit { dx: f32, dy: f32 },
    /// Move the view by world units along its screen basis; the 2D front view
    /// uses world X/Y. Positive `dx` moves the view right.
    Pan { dx: f32, dy: f32 },
    /// Multiplicative zoom; `> 1` zooms in (egui's wheel convention).
    Zoom { factor: f32 },
    /// The default framing: default orientation, distance and zoom, centred on
    /// the domain.
    Reset,
    /// Frame the whole domain: keep the orbit orientation (3D) or the front
    /// view (2D), centre on the domain, and pull back / zoom out to fit it.
    Fit,
}

/// The centre of the domain's box. Reset and fit both centre on it, so a
/// domain dragged off the origin still comes into view.
pub fn domain_center(domain: Domain) -> Vec3 {
    domain.center()
}

/// How far the domain's centre moved from `old` to `new`. Ticket 47 translates
/// the fly camera by this so the free camera keeps its relative view.
pub fn domain_centre_delta(old: &Domain, new: &Domain) -> Vec3 {
    new.center() - old.center()
}

/// Orbit distance that fits the domain's bounding sphere in the viewport's
/// tighter field of view, clamped to the camera's usable range. The vertical
/// FOV limits a landscape viewport; a portrait/narrow viewport is limited by
/// its horizontal FOV (`2*atan(tan(fov_y/2)*aspect)`), so Fit pulls back far
/// enough to keep the whole domain visible. Fitting the sphere also keeps the
/// domain on screen at any orbit angle, and the answer is deterministic for
/// square and rectangular boxes alike.
pub fn fit_distance(domain: Domain, fov_y: f32, aspect: f32) -> f32 {
    let radius = domain.domain_size().length() * 0.5;
    let fov_y = if fov_y.is_finite() && fov_y > 0.0 {
        fov_y
    } else {
        OrbitCamera::default().fov_y
    };
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    let half_y = fov_y * 0.5;
    let half_x = (half_y.tan() * aspect).atan();
    let half_fov = half_y.min(half_x);
    let sin_half = half_fov.sin().max(1e-4);
    let distance = radius / sin_half;
    if distance.is_finite() {
        distance.clamp(OrbitCamera::MIN_DISTANCE, OrbitCamera::MAX_DISTANCE)
    } else {
        OrbitCamera::default().distance
    }
}

/// The visible world height at the orbit target, used to scale a keyboard pan
/// like the picture under the pointer. 2D uses the same padded plane size the
/// viewport and click seeding share.
fn visible_height(camera: &OrbitCamera, view: ViewContext) -> f32 {
    if view.mode_3d {
        let fov_y = if camera.fov_y.is_finite() && camera.fov_y > 0.0 {
            camera.fov_y
        } else {
            OrbitCamera::default().fov_y
        };
        2.0 * camera.distance * (fov_y * 0.5).tan()
    } else {
        let size = view.domain.domain_size();
        let zoom = if camera.ortho_zoom.is_finite() && camera.ortho_zoom > 0.0 {
            camera.ortho_zoom
        } else {
            1.0
        };
        (size.x.max(size.y) * PLANE_MARGIN) / zoom
    }
}

/// Apply `command` to `camera` for `view` and return the new camera. Pure: the
/// caller owns the camera and decides when to store the result.
pub fn apply_view_command(
    camera: OrbitCamera,
    view: ViewContext,
    command: ViewCommand,
) -> OrbitCamera {
    let mut camera = camera;
    match command {
        ViewCommand::Orbit { dx, dy } => {
            if view.mode_3d {
                camera.orbit(finite_or_zero(dx), finite_or_zero(dy));
            }
        }
        ViewCommand::Pan { dx, dy } => {
            let (dx, dy) = (finite_or_zero(dx), finite_or_zero(dy));
            if view.mode_3d {
                camera.pan(dx, dy);
            } else {
                // The 2D view is a fixed front view, so its screen basis is
                // world X/Y regardless of the orbit angles.
                camera.target.x += dx;
                camera.target.y += dy;
            }
        }
        ViewCommand::Zoom { factor } => {
            if factor.is_finite() && factor > 0.0 {
                if view.mode_3d {
                    camera.zoom(1.0 / factor);
                } else {
                    camera.zoom_2d(factor);
                }
            }
        }
        ViewCommand::Reset => {
            // Issue #208: reset restores the default pose but keeps the user's
            // display-only up axis; it is not a document/camera preference.
            camera = OrbitCamera {
                target: domain_center(view.domain),
                up_axis: camera.up_axis,
                ..OrbitCamera::default()
            };
        }
        ViewCommand::Fit => {
            camera.target = domain_center(view.domain);
            if view.mode_3d {
                camera.distance = fit_distance(view.domain, camera.fov_y, view.aspect);
            } else {
                // The 2D ortho projection shows `plane_size / zoom` vertically
                // and that times the aspect horizontally. Fit starts at the
                // padded plane (zoom 1) and only zooms out when a narrow
                // viewport would clip the domain's width.
                let size = view.domain.domain_size();
                let width = size.x.max(1e-6);
                let height = size.y.max(1e-6);
                let plane_size = width.max(height) * PLANE_MARGIN;
                let zoom = (plane_size / height)
                    .min(plane_size * view.aspect / width)
                    .min(1.0);
                camera.ortho_zoom =
                    zoom.clamp(OrbitCamera::MIN_ORTHO_ZOOM, OrbitCamera::MAX_ORTHO_ZOOM);
            }
        }
    }
    camera
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// The app-level camera command: Reset and Fit leave fly mode first and then
/// act on the untouched orbit camera; orbit/pan/zoom commands are inert while
/// flying, so the pre-fly orbit view really does come back on exit. Returns the
/// new orbit camera and drops `fly` when the command takes over the view.
pub fn apply_view_command_with_fly(
    camera: OrbitCamera,
    fly: &mut Option<FlyCamera>,
    view: ViewContext,
    command: ViewCommand,
) -> OrbitCamera {
    if fly.is_some() {
        match command {
            ViewCommand::Reset | ViewCommand::Fit => *fly = None,
            // WASD/drag drive the fly camera; orbit commands must not mutate
            // the hidden orbit pose they would otherwise restore.
            _ => return camera,
        }
    }
    apply_view_command(camera, view, command)
}

/// Convert egui's platform modifiers into the profile's logical chord set.
/// `Mod` is egui's `command` (Command on macOS, Ctrl elsewhere).
pub fn chord_modifiers(modifiers: egui::Modifiers) -> ChordModifiers {
    let command = if cfg!(target_os = "macos") {
        modifiers.command || modifiers.mac_cmd
    } else {
        // Some backends set `ctrl` without `command`; both mean Mod here.
        modifiers.command || modifiers.ctrl
    };
    ChordModifiers {
        command,
        shift: modifiers.shift,
        alt: modifiers.alt,
    }
}

/// Convert a profile chord into egui's exact modifier set.
pub fn egui_modifiers(chord: KeyChord) -> egui::Modifiers {
    let mut modifiers = egui::Modifiers::NONE;
    if chord.modifiers.command {
        modifiers = modifiers.plus(egui::Modifiers::COMMAND);
    }
    if chord.modifiers.shift {
        modifiers = modifiers.plus(egui::Modifiers::SHIFT);
    }
    if chord.modifiers.alt {
        modifiers = modifiers.plus(egui::Modifiers::ALT);
    }
    modifiers
}

/// The action exactly bound to this key and modifier set, if any. No aliases
/// and no shift tolerance: menu, transport and tool chords are exact.
pub fn bound_action(
    bindings: &Bindings,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<InteractionAction> {
    bindings.owner_of(KeyChord {
        key,
        modifiers: chord_modifiers(modifiers),
    })
}

/// Whether `action` keeps its meaning when Shift is held. View arrows use
/// Shift for pan (3D) or faster pan (2D), and `+`/`-` arrive as Shift+Equals
/// and Shift+Minus on many layouts, so those chords accept an extra Shift
/// beyond their bound set.
fn shift_tolerant(action: InteractionAction) -> bool {
    matches!(
        action,
        InteractionAction::ZoomIn
            | InteractionAction::ZoomOut
            | InteractionAction::ViewLeft
            | InteractionAction::ViewRight
            | InteractionAction::ViewUp
            | InteractionAction::ViewDown
    )
}

/// Resolve a key press through the active bindings for the camera/view keys.
/// Unlike [`bound_action`] this accepts an extra Shift for the view actions and
/// treats `=` as the same physical key as `+` while ZoomIn still owns `+`.
pub fn view_action_for_key(
    bindings: &Bindings,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<InteractionAction> {
    let mut mods = chord_modifiers(modifiers);
    if let Some(action) = bindings.owner_of(KeyChord {
        key,
        modifiers: mods,
    }) {
        return Some(action);
    }

    // `=` is the unshifted physical key for `+` on many layouts. Only accept
    // the alias while ZoomIn still owns `Plus`, so remapping ZoomIn removes it.
    if key == egui::Key::Equals
        && let Some(action) = bindings.owner_of(KeyChord {
            key: egui::Key::Plus,
            modifiers: mods,
        })
        && action == InteractionAction::ZoomIn
    {
        return Some(action);
    }

    if mods.shift {
        mods.shift = false;
        if let Some(action) = bindings.owner_of(KeyChord {
            key,
            modifiers: mods,
        }) && shift_tolerant(action)
        {
            return Some(action);
        }
        if key == egui::Key::Equals
            && let Some(action) = bindings.owner_of(KeyChord {
                key: egui::Key::Plus,
                modifiers: mods,
            })
            && action == InteractionAction::ZoomIn
        {
            return Some(action);
        }
    }
    None
}

/// The command one resolved view action stands for. `shift` is the frame's
/// actual Shift state, not merely part of the bound chord.
pub fn command_for_action(
    action: InteractionAction,
    camera: &OrbitCamera,
    view: ViewContext,
    shift: bool,
    feel: CameraFeel,
) -> Option<ViewCommand> {
    let mut pan = visible_height(camera, view) * KEY_PAN_FRACTION;
    if !view.mode_3d && shift {
        pan *= KEY_PAN_FAST_MULTIPLIER;
    }
    let orbit = |dx: f32, dy: f32| {
        let delta = crate::camera::feel_orbit(glam::Vec2::new(dx, dy), feel);
        ViewCommand::Orbit {
            dx: delta.x,
            dy: delta.y,
        }
    };
    match action {
        InteractionAction::ResetView => Some(ViewCommand::Reset),
        InteractionAction::FitView => Some(ViewCommand::Fit),
        InteractionAction::ZoomIn => Some(ViewCommand::Zoom {
            factor: KEY_ZOOM_FACTOR,
        }),
        InteractionAction::ZoomOut => Some(ViewCommand::Zoom {
            factor: 1.0 / KEY_ZOOM_FACTOR,
        }),
        InteractionAction::ViewLeft => Some(if view.mode_3d && !shift {
            orbit(-KEY_ORBIT_STEP, 0.0)
        } else {
            ViewCommand::Pan { dx: -pan, dy: 0.0 }
        }),
        InteractionAction::ViewRight => Some(if view.mode_3d && !shift {
            orbit(KEY_ORBIT_STEP, 0.0)
        } else {
            ViewCommand::Pan { dx: pan, dy: 0.0 }
        }),
        InteractionAction::ViewUp => Some(if view.mode_3d && !shift {
            orbit(0.0, KEY_ORBIT_STEP)
        } else {
            ViewCommand::Pan { dx: 0.0, dy: pan }
        }),
        InteractionAction::ViewDown => Some(if view.mode_3d && !shift {
            orbit(0.0, -KEY_ORBIT_STEP)
        } else {
            ViewCommand::Pan { dx: 0.0, dy: -pan }
        }),
        _ => None,
    }
}

/// The command one camera key stands for with the default bindings. Retained
/// as the pure keyboard seam's default-profile entry point; the app passes the
/// live bindings through [`command_for_key_with_bindings`].
pub fn command_for_key(
    camera: &OrbitCamera,
    view: ViewContext,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<ViewCommand> {
    command_for_key_with_bindings(
        camera,
        view,
        &Bindings::default_bindings(),
        CameraFeel::default(),
        key,
        modifiers,
    )
}

/// The command one key press stands for under `bindings`.
pub fn command_for_key_with_bindings(
    camera: &OrbitCamera,
    view: ViewContext,
    bindings: &Bindings,
    feel: CameraFeel,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<ViewCommand> {
    let action = view_action_for_key(bindings, key, modifiers)?;
    command_for_action(action, camera, view, modifiers.shift, feel)
}

/// Consume this frame's camera keys and report the commands to apply, in
/// [`InteractionAction::REMAPPABLE`] order. Inert while a text field has focus,
/// so typing in the formula bar keeps its arrows, letters and `+`/`-` (the
/// menu's global shortcuts run in their own loop before this). Keys are
/// consumed so no other widget sees a key the camera already handled.
pub fn view_key_commands(
    ctx: &egui::Context,
    camera: &OrbitCamera,
    view: ViewContext,
) -> Vec<ViewCommand> {
    view_key_commands_with_bindings(
        ctx,
        camera,
        view,
        &Bindings::default_bindings(),
        CameraFeel::default(),
    )
}

/// Consume this frame's camera keys through the active bindings.
pub fn view_key_commands_with_bindings(
    ctx: &egui::Context,
    camera: &OrbitCamera,
    view: ViewContext,
    bindings: &Bindings,
    feel: CameraFeel,
) -> Vec<ViewCommand> {
    if ctx.text_edit_focused() {
        return Vec::new();
    }
    let mut commands = Vec::new();
    ctx.input_mut(|input| {
        let modifiers = input.modifiers;
        for action in [
            InteractionAction::ResetView,
            InteractionAction::FitView,
            InteractionAction::ZoomIn,
            InteractionAction::ZoomOut,
            InteractionAction::ViewLeft,
            InteractionAction::ViewRight,
            InteractionAction::ViewUp,
            InteractionAction::ViewDown,
        ] {
            let Some(chord) = bindings.chord(action) else {
                continue;
            };
            let mut keys = [Some(chord.key), None];
            if action == InteractionAction::ZoomIn && chord.key == egui::Key::Plus {
                keys[1] = Some(egui::Key::Equals);
            }
            let mut matched = None;
            for key in keys.into_iter().flatten() {
                if input.key_pressed(key)
                    && view_action_for_key(bindings, key, modifiers) == Some(action)
                {
                    matched = Some(key);
                    break;
                }
            }
            let Some(key) = matched else {
                continue;
            };
            if let Some(command) = command_for_action(action, camera, view, modifiers.shift, feel)
                && input.consume_key(modifiers, key)
            {
                commands.push(command);
            }
        }
    });
    commands
}

/// The one-line status hint for a left click that cannot seed because the
/// Streamlines feature is off (ticket 60).
pub const CLICK_HINT_STREAMLINES_OFF: &str = "Streamlines are off";
/// The one-line status hint for AddParticles with the feature off.
pub const CLICK_HINT_PARTICLES_OFF: &str = "Particles are off";
/// The one-line status hint for AddParticles when no particle cloud can exist:
/// the field has no third component to slice or orbit.
pub const CLICK_HINT_PARTICLES_NEED_3D: &str = "Particles need a 3D field";

/// What one explicit click action should do after the feature gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickActionPlan {
    /// Store one click streamline seed; never add a particle burst.
    SeedStreamline,
    /// Add the existing particle burst; never store a click seed.
    AddParticles,
    /// The action's feature is off or the field has no particle view: do
    /// nothing and show `hint` in the status bar.
    Ignore(&'static str),
}

/// Decide what one click does from the selected action, the Streamlines toggle
/// and particle availability (ticket 60; plane slices in ticket 13). Pure so
/// the mouse click and both keyboard shortcuts share exactly one rule.
///
/// `particles_available` is the whole particle gate — the toggle is on *and*
/// the field has three components — so a 2D plane slice allows a burst just
/// like the 3D view. `mode_3d` only picks the no-op hint when availability is
/// false: in the 3D view the toggle is off, while in 2D the field itself lacks
/// a third component.
pub fn click_action_plan(
    action: LeftClickAction,
    streamlines: bool,
    particles_available: bool,
    mode_3d: bool,
) -> ClickActionPlan {
    match action {
        LeftClickAction::SeedStreamline if streamlines => ClickActionPlan::SeedStreamline,
        LeftClickAction::SeedStreamline => ClickActionPlan::Ignore(CLICK_HINT_STREAMLINES_OFF),
        LeftClickAction::AddParticles if !particles_available && mode_3d => {
            ClickActionPlan::Ignore(CLICK_HINT_PARTICLES_OFF)
        }
        LeftClickAction::AddParticles if !particles_available => {
            ClickActionPlan::Ignore(CLICK_HINT_PARTICLES_NEED_3D)
        }
        LeftClickAction::AddParticles => ClickActionPlan::AddParticles,
    }
}

/// Map one plain S/P key press to the click action it runs (ticket 60). S seeds
/// a streamline and P adds a particle burst, regardless of the selected
/// left-click action.
///
/// The mapping is deliberately pure and only encodes the keyboard gates:
/// feature availability is [`click_action_plan`]'s job. A text field keeps every
/// key, OS key-repeat fires once per physical press, and only plain modifiers
/// count (Ctrl/Alt/Command are menu/browser chords; Shift+S is not a seed).
/// S is inert while flying because it is the fly camera's backward key; P is
/// unused by fly and stays available.
pub fn seed_shortcut(
    key: egui::Key,
    modifiers: egui::Modifiers,
    repeat: bool,
    text_edit_focused: bool,
    flying: bool,
) -> Option<LeftClickAction> {
    seed_shortcut_with_bindings(
        &Bindings::default_bindings(),
        key,
        modifiers,
        repeat,
        text_edit_focused,
        flying,
    )
}

/// [`seed_shortcut`] under the active bindings: whichever chord owns
/// SeedStreamline or AddParticles runs that action. The gates stay the same;
/// Seed is inert while flying when it is still on physical `S`, because that
/// key belongs to the fly camera's backward movement (ticket 70).
pub fn seed_shortcut_with_bindings(
    bindings: &Bindings,
    key: egui::Key,
    modifiers: egui::Modifiers,
    repeat: bool,
    text_edit_focused: bool,
    flying: bool,
) -> Option<LeftClickAction> {
    if text_edit_focused || repeat {
        return None;
    }
    let action = bound_action(bindings, key, modifiers)?;
    match action {
        InteractionAction::SeedStreamline if !(flying && key == egui::Key::S) => {
            Some(LeftClickAction::SeedStreamline)
        }
        InteractionAction::AddParticles => Some(LeftClickAction::AddParticles),
        _ => None,
    }
}

/// The point a keyboard seed/burst lands on: the pointer when it is inside the
/// viewport, otherwise the viewport centre. A captured/locked pointer may not
/// hover at all, which is why `pointer` is optional.
pub fn shortcut_seed_point(pointer: Option<egui::Pos2>, viewport_rect: egui::Rect) -> egui::Pos2 {
    match pointer {
        Some(pos) if viewport_rect.contains(pos) => pos,
        _ => viewport_rect.center(),
    }
}
