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
use crate::viewmodel::PLANE_MARGIN;

/// Radians the view orbits per arrow-key press in 3D.
pub const KEY_ORBIT_STEP: f32 = 0.06;
/// A keyboard pan moves this fraction of the visible height per press.
pub const KEY_PAN_FRACTION: f32 = 0.05;
/// Shift+arrow in 2D pans this many times faster than a plain arrow.
pub const KEY_PAN_FAST_MULTIPLIER: f32 = 3.0;
/// Multiplicative zoom per `+`/`-` press (egui's convention: >1 gets closer).
pub const KEY_ZOOM_FACTOR: f32 = 1.2;
/// The keys the camera consumes. Listed in one place so the input loop and the
/// tests cannot drift; menu shortcuts are a separate table.
pub const CAMERA_KEYS: &[egui::Key] = &[
    egui::Key::ArrowLeft,
    egui::Key::ArrowRight,
    egui::Key::ArrowUp,
    egui::Key::ArrowDown,
    egui::Key::Plus,
    egui::Key::Equals,
    egui::Key::Minus,
    egui::Key::R,
    egui::Key::F,
];

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
    (domain.min + domain.max) * 0.5
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
pub fn apply_view_command(camera: OrbitCamera, view: ViewContext, command: ViewCommand) -> OrbitCamera {
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
            camera = OrbitCamera {
                target: domain_center(view.domain),
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
                camera.ortho_zoom = zoom.clamp(OrbitCamera::MIN_ORTHO_ZOOM, OrbitCamera::MAX_ORTHO_ZOOM);
            }
        }
    }
    camera
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// The command one camera key stands for, if any. `modifiers` must not carry
/// Ctrl/Cmd/Alt: those keys belong to menus and the browser (`Ctrl+R` reload,
/// `Ctrl+F` find). Shift is meaningful for arrows (3D: pan; 2D: pan faster)
/// and for `+`.
pub fn command_for_key(
    camera: &OrbitCamera,
    view: ViewContext,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<ViewCommand> {
    if modifiers.ctrl || modifiers.command || modifiers.alt || modifiers.mac_cmd {
        return None;
    }
    let shift = modifiers.shift;
    let mut pan = visible_height(camera, view) * KEY_PAN_FRACTION;
    if !view.mode_3d && shift {
        pan *= KEY_PAN_FAST_MULTIPLIER;
    }
    match key {
        egui::Key::ArrowLeft => Some(if view.mode_3d && !shift {
            ViewCommand::Orbit { dx: -KEY_ORBIT_STEP, dy: 0.0 }
        } else {
            ViewCommand::Pan { dx: -pan, dy: 0.0 }
        }),
        egui::Key::ArrowRight => Some(if view.mode_3d && !shift {
            ViewCommand::Orbit { dx: KEY_ORBIT_STEP, dy: 0.0 }
        } else {
            ViewCommand::Pan { dx: pan, dy: 0.0 }
        }),
        egui::Key::ArrowUp => Some(if view.mode_3d && !shift {
            ViewCommand::Orbit { dx: 0.0, dy: KEY_ORBIT_STEP }
        } else {
            ViewCommand::Pan { dx: 0.0, dy: pan }
        }),
        egui::Key::ArrowDown => Some(if view.mode_3d && !shift {
            ViewCommand::Orbit { dx: 0.0, dy: -KEY_ORBIT_STEP }
        } else {
            ViewCommand::Pan { dx: 0.0, dy: -pan }
        }),
        // `+` reaches egui as Plus on some layouts and Shift+Equals on others;
        // a bare `=` is accepted as zoom-in too.
        egui::Key::Plus | egui::Key::Equals => Some(ViewCommand::Zoom { factor: KEY_ZOOM_FACTOR }),
        egui::Key::Minus => Some(ViewCommand::Zoom { factor: 1.0 / KEY_ZOOM_FACTOR }),
        egui::Key::R if !shift => Some(ViewCommand::Reset),
        egui::Key::F if !shift => Some(ViewCommand::Fit),
        _ => None,
    }
}

/// Consume this frame's camera keys and report the commands to apply, in
/// [`CAMERA_KEYS`] order. Inert while a text field has focus, so typing in the
/// formula bar keeps its arrows, letters and `+`/`-` (the menu's global
/// shortcuts run in their own loop before this). Keys are consumed so no other
/// widget sees a key the camera already handled.
pub fn view_key_commands(
    ctx: &egui::Context,
    camera: &OrbitCamera,
    view: ViewContext,
) -> Vec<ViewCommand> {
    if ctx.text_edit_focused() {
        return Vec::new();
    }
    let mut commands = Vec::new();
    ctx.input_mut(|input| {
        let modifiers = input.modifiers;
        for key in CAMERA_KEYS {
            if !input.key_pressed(*key) {
                continue;
            }
            if let Some(command) = command_for_key(camera, view, *key, modifiers) {
                input.consume_key(modifiers, *key);
                commands.push(command);
            }
        }
    });
    commands
}
