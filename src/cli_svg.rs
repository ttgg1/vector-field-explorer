//! Pure vector serialization for native CLI exports.
use std::fmt::Write;

use glam::{Vec2, Vec3, Vec4};

use super::{CliError, OutputFrame, PreparedScene};
use crate::{arrow_style::ArrowStyle2D, renderer::LineInstanceGPU};

/// Serialize the prepared 2D scene without creating GPU resources.
/// World-space silhouettes and straight alpha match the 2D scene controls;
/// this vector export does not reproduce GPU antialiasing or dot lighting.
pub fn scene_to_cli_svg(prepared: &PreparedScene) -> Result<String, CliError> {
    let scene = &prepared.scene;
    if scene.mode_3d {
        return Err(CliError::Validation(
            "3D SVG is unsupported; choose PNG".into(),
        ));
    }
    let frame = &prepared.frame;
    let mut svg = Svg {
        frame,
        out: String::new(),
    };
    let [r, g, b, a] = frame.background_srgb;
    write!(svg.out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"#{r:02x}{g:02x}{b:02x}\" fill-opacity=\"{:.6}\"/>",
        frame.width, frame.height, frame.width, frame.height, f32::from(a) / 255.0,
    ).unwrap();

    // Preserve the renderer's painter order and the prepared instance order.
    for line in scene.lines.iter() {
        svg.line(line, "scene-line");
    }
    for line in scene.trails.iter() {
        svg.line(line, "trail");
    }
    let params = scene.arrow_params.sanitized();
    for arrow in scene.arrows.iter() {
        let base = Vec2::new(arrow.origin[0], arrow.origin[1]);
        let mut axis = Vec2::new(arrow.dir_len[0], arrow.dir_len[1]);
        axis = if axis.length() < 1e-5 {
            Vec2::Y
        } else {
            axis.normalize()
        };
        let side = Vec2::new(-axis.y, axis.x);
        let length = arrow.dir_len[3];
        let headless = scene.arrow_style_2d == ArrowStyle2D::Streamline;
        let junction = base + axis * length * (1.0 - params.head_length);
        let tip = base + axis * length;
        let end = if headless { tip } else { junction };
        let half_shaft = side * params.shaft_radius * length;
        let world = |p: Vec2| p.extend(arrow.origin[2]);
        svg.polygon(
            &[
                world(base + half_shaft),
                world(end + half_shaft),
                world(end - half_shaft),
                world(base - half_shaft),
            ],
            arrow.color,
            "arrow-shaft",
        );
        if !headless {
            let half_head = side * params.head_width * length;
            svg.polygon(
                &[
                    world(junction + half_head),
                    world(junction - half_head),
                    world(tip),
                ],
                arrow.color,
                "arrow-head",
            );
        }
    }
    for dot in scene.dots.iter() {
        // 2D front-view billboards lie in XY. Approximate the round silhouette
        // with a vector polygon so partial discs use the same geometric clipper.
        // Prepared CLI 2D scenes normally contain no particles (State eligibility).
        const DISC_SIDES: usize = 64;
        let center = Vec3::new(dot.origin[0], dot.origin[1], dot.origin[2]);
        let vertices: Vec<_> = (0..DISC_SIDES)
            .map(|i| {
                let angle = std::f32::consts::TAU * i as f32 / DISC_SIDES as f32;
                let (sin, cos) = angle.sin_cos();
                center + Vec3::new(cos, sin, 0.0) * dot.dir_len[3]
            })
            .collect();
        svg.polygon(&vertices, dot.color, "particle");
    }
    svg.out.push_str("</svg>");
    Ok(svg.out)
}

struct Svg<'a> {
    frame: &'a OutputFrame,
    out: String,
}

impl Svg<'_> {
    fn line(&mut self, line: &LineInstanceGPU, class: &str) {
        let a = Vec3::new(line.a[0], line.a[1], line.a[2]);
        let b = Vec3::new(line.b[0], line.b[1], line.b[2]);
        // Match shaders.line.wgsl: width is in world units, not pixels. Clip
        // the whole ribbon rather than dropping segments with offscreen ends.
        let along = (b - a).normalize_or_zero();
        let to_camera = (self.frame.eye - a).normalize_or_zero();
        let half = along.cross(to_camera).normalize_or_zero() * line.b[3] * 0.5;
        self.polygon(&[a + half, b + half, b - half, a - half], line.color, class);
    }

    fn polygon(&mut self, world: &[Vec3], color: [f32; 4], class: &str) {
        let mut vertices: Vec<_> = world
            .iter()
            .map(|p| self.frame.view_proj * p.extend(1.0))
            .collect();
        if !vertices.iter().all(|v| v.is_finite()) || !color.iter().all(|c| c.is_finite()) {
            return;
        }
        // Sutherland-Hodgman in homogeneous clip space. WebGPU depth is 0..w;
        // clipping before division preserves both crossing ribbons and polygons
        // whose original vertices are all outside the output rectangle.
        for plane in 0..6 {
            vertices = clip_polygon(&vertices, plane);
            if vertices.len() < 3 {
                return;
            }
        }
        let screen: Vec<_> = vertices
            .iter()
            .filter(|v| v.w > 0.0)
            .map(|v| {
                let ndc = v.truncate() / v.w;
                Vec2::new(
                    ((ndc.x + 1.0) * 0.5 * self.frame.width as f32)
                        .clamp(0.0, self.frame.width as f32),
                    ((1.0 - ndc.y) * 0.5 * self.frame.height as f32)
                        .clamp(0.0, self.frame.height as f32),
                )
            })
            .collect();
        if screen.len() < 3 {
            return;
        }
        let area: f32 = screen
            .iter()
            .zip(screen.iter().cycle().skip(1))
            .map(|(a, b)| a.perp_dot(*b))
            .sum();
        if area.abs() <= f32::EPSILON {
            return;
        }
        write!(self.out, "<polygon class=\"{class}\" points=\"").unwrap();
        for (i, p) in screen.iter().enumerate() {
            if i > 0 {
                self.out.push(' ');
            }
            write!(self.out, "{:.4},{:.4}", p.x, p.y).unwrap();
        }
        write!(
            self.out,
            "\" fill=\"#{:02x}{:02x}{:02x}\" fill-opacity=\"{:.6}\"/>",
            display_byte(color[0]),
            display_byte(color[1]),
            display_byte(color[2]),
            color[3].clamp(0.0, 1.0),
        )
        .unwrap();
    }
}

fn clip_polygon(vertices: &[Vec4], plane: usize) -> Vec<Vec4> {
    let distance = |p: Vec4| match plane {
        0 => p.w + p.x,
        1 => p.w - p.x,
        2 => p.w + p.y,
        3 => p.w - p.y,
        4 => p.z,
        _ => p.w - p.z,
    };
    let mut clipped = Vec::new();
    let Some(&last) = vertices.last() else {
        return clipped;
    };
    let mut previous = last;
    let mut previous_distance = distance(previous);
    for &current in vertices {
        let current_distance = distance(current);
        if (previous_distance >= 0.0) != (current_distance >= 0.0) {
            let fraction = previous_distance / (previous_distance - current_distance);
            clipped.push(previous.lerp(current, fraction));
        }
        if current_distance >= 0.0 {
            clipped.push(current);
        }
        previous = current;
        previous_distance = current_distance;
    }
    clipped
}

fn display_byte(linear: f32) -> u8 {
    let linear = linear.clamp(0.0, 1.0);
    let srgb = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}
