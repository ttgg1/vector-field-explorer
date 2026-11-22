//! Egui adapter for `katex-native` display lists (issue #171).
//!
//! The adapter is deliberately total: every parse/layout failure and every
//! unsupported display item returns `None` so the CAS panel can keep showing
//! the canonical linear string. Only shapes egui paints exactly are emitted:
//! text, filled rectangles, line segments, and convex filled paths. KaTeX
//! radicals and other stretchy paths fall back to linear output.
//!
//! `KatexContext::default()` builds function and symbol tables, so it is built
//! once in a process-wide [`OnceLock`] instead of per frame or per panel.

use std::sync::OnceLock;

use eframe::egui::{
    self, Color32, FontData, FontFamily, FontId, FontInsert, FontPriority, InsertFontFamily, Pos2,
    Shape, Stroke, Vec2,
};
use katex_native::{
    FONTS, Font, FontFile, Item, PathCommand, Rect as KatexRect,
    katex::{KatexContext, Settings},
};

/// The shared katex-rs context. Building it is table construction, not layout.
pub fn shared_context() -> &'static KatexContext {
    static CONTEXT: OnceLock<KatexContext> = OnceLock::new();
    CONTEXT.get_or_init(KatexContext::default)
}

/// Register every KaTeX face as a named egui family. Idempotent: egui dedups
/// `add_font` by font name, so startup and per-item registration can coexist.
/// Callers invoke this once on the app context before the first frame (through
/// `fonts::install_startup_fonts`); the per-item call then only covers a face
/// the startup set missed.
pub fn install_math_fonts(ctx: &egui::Context) {
    for face in FONTS {
        install_face(ctx, face);
    }
}

/// A laid-out formula converted to egui shapes, in points and relative to the
/// top-left of its bounding box. The baseline sits `height` points from the top.
#[derive(Clone, Debug, Default)]
pub struct MathLayout {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    shapes: Vec<Shape>,
}

impl MathLayout {
    /// True when the formula produced nothing paintable.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// Bounding-box size, including the depth below the baseline.
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width, self.height + self.depth)
    }

    /// Paint relative to `top_left`; the painter's clip is respected by egui.
    pub fn paint(&self, painter: &egui::Painter, top_left: Pos2) {
        let offset = top_left.to_vec2();
        for shape in &self.shapes {
            let mut shape = shape.clone();
            shape.translate(offset);
            painter.add(shape);
        }
    }
}

/// Lay out `tex` and convert the display list into egui shapes.
///
/// Returns `None` for a parse/layout error, a panic inside the layout engine,
/// a font face with no available fallback, or a display item this adapter
/// cannot paint exactly.
///
/// `Layout.tag` and `Layout.equation_numbers` are deliberately ignored: the
/// adapter's settings never enable equation numbering, and the display printer
/// never emits `\tag`.
pub fn layout_math(ui: &mut egui::Ui, tex: &str, context: &KatexContext) -> Option<MathLayout> {
    if tex.is_empty() {
        return None;
    }
    let settings = Settings::builder().display_mode(true).build();
    let layout = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        katex_native::layout_with(context, tex, &settings)
    }))
    .ok()?
    .ok()?;
    convert_items(ui, layout.width, layout.height, layout.depth, &layout.items)
}

/// Test seam for the display-list conversion, including the finiteness screen.
#[doc(hidden)]
pub fn math_layout_from_items(
    ui: &mut egui::Ui,
    width: f64,
    height: f64,
    depth: f64,
    items: &[Item],
) -> Option<MathLayout> {
    convert_items(ui, width, height, depth, items)
}

fn convert_items(
    ui: &mut egui::Ui,
    width: f64,
    height: f64,
    depth: f64,
    items: &[Item],
) -> Option<MathLayout> {
    let em = body_font_size(ui);
    let em = if em.is_finite() { em.max(1.0) } else { 14.0 } as f64;
    let width = scaled(width, em).filter(|value| *value >= 0.0)?;
    let height = scaled(height, em).filter(|value| *value >= 0.0)?;
    let depth = scaled(depth, em).filter(|value| *value >= 0.0)?;

    let base_color = ui.visuals().text_color();
    let mut shapes = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Item::Text {
                x,
                y,
                text,
                font,
                size,
                color,
            } => {
                if !size.is_finite() || *size <= 0.0 {
                    return None;
                }
                let x = scaled(*x, em)?;
                let y = scaled(*y, em)?;
                let size = scaled(*size, em).filter(|value| *value > 0.0)?;
                let face = face_for(*font)?;
                install_face(ui.ctx(), face);
                let font_id = FontId::new(size, family_for(face));
                let color = color_or(*color, base_color);
                let galley = ui
                    .ctx()
                    .fonts_mut(|fonts| fonts.layout_no_wrap(text.clone(), font_id, color));
                let ascent = galley
                    .rows
                    .first()
                    .and_then(|row| row.row.glyphs.first())
                    .map(|glyph| glyph.font_ascent)
                    .unwrap_or(size * 0.8);
                let pos = Pos2::new(x, y - ascent);
                if !pos.x.is_finite() || !pos.y.is_finite() {
                    return None;
                }
                shapes.push(Shape::Text(egui::epaint::TextShape::new(
                    pos, galley, color,
                )));
            }
            Item::Rect {
                x,
                y,
                width,
                height,
                color,
            } => {
                if !width.is_finite() || !height.is_finite() || *width < 0.0 || *height < 0.0 {
                    return None;
                }
                let x = scaled(*x, em)?;
                let y = scaled(*y, em)?;
                let width = scaled(*width, em)?;
                let height = scaled(*height, em)?;
                let color = color_or(*color, base_color);
                shapes.push(Shape::rect_filled(
                    egui::Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height)),
                    0.0,
                    color,
                ));
            }
            Item::Line {
                from,
                to,
                thickness,
                color,
            } => {
                if !thickness.is_finite() || *thickness < 0.0 {
                    return None;
                }
                let from = (scaled(from.0, em)?, scaled(from.1, em)?);
                let to = (scaled(to.0, em)?, scaled(to.1, em)?);
                let thickness = scaled(*thickness, em)?;
                let color = color_or(*color, base_color);
                shapes.push(Shape::line_segment(
                    [Pos2::new(from.0, from.1), Pos2::new(to.0, to.1)],
                    Stroke::new(thickness, color),
                ));
            }
            Item::Path {
                commands,
                clip,
                color,
            } => {
                // Multiple contour starts would need separate polygons; the
                // adapter deliberately falls back rather than join them.
                if commands
                    .iter()
                    .filter(|command| matches!(command, PathCommand::MoveTo(..)))
                    .count()
                    > 1
                {
                    return None;
                }
                let points = flatten_path(commands, em);
                if !convex(&points) || !clip_contains(*clip, commands) {
                    return None;
                }
                let color = color_or(*color, base_color);
                shapes.push(Shape::convex_polygon(points, color, Stroke::NONE));
            }
            // `Item` is non-exhaustive: an item added by a future katex-native
            // release must fall back rather than be dropped or panic.
            _ => return None,
        }
    }
    if shapes.is_empty() {
        return None;
    }
    Some(MathLayout {
        width,
        height,
        depth,
        shapes,
    })
}

/// Scale an ems value to points, rejecting non-finite input and f32 overflow.
fn scaled(value: f64, em: f64) -> Option<f32> {
    let value = value * em;
    (value.is_finite() && value.abs() <= f32::MAX as f64).then_some(value as f32)
}

/// Paint `tex` if it lays out, otherwise paint `linear` with the fallback note.
/// Returns `true` only when typeset shapes were painted.
pub fn paint_tex_or_linear(ui: &mut egui::Ui, tex: Option<&str>, linear: &str) -> bool {
    if let Some(tex) = tex
        && let Some(layout) = layout_math(ui, tex, shared_context())
    {
        // A clipped fragment is less honest than the canonical linear string.
        // The panel is inside a scroll area, so the height check is skipped
        // when egui reports an unbounded available height.
        let available = ui.available_size();
        if layout.width > available.x || layout.size().y > available.y {
            show_linear(ui, linear);
            ui.weak("Showing linear result");
            return false;
        }
        let (response, painter) = ui.allocate_painter(layout.size(), egui::Sense::hover());
        layout.paint(&painter, response.rect.min);
        return true;
    }
    show_linear(ui, linear);
    ui.weak("Showing linear result");
    false
}

/// The canonical linear result, kept selectable and accessible.
pub fn show_linear(ui: &mut egui::Ui, linear: &str) {
    let mut result = linear.to_owned();
    ui.add(
        egui::TextEdit::multiline(&mut result)
            .desired_width(ui.available_width())
            .desired_rows(2)
            .interactive(false),
    );
}

fn body_font_size(ui: &egui::Ui) -> f32 {
    ui.style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(14.0, |font| font.size)
}

fn color_or(color: Option<katex_native::Color>, fallback: Color32) -> Color32 {
    color.map_or(fallback, |color| {
        Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
    })
}

/// Pick the exact face, then a style-compatible sibling, then Main.
fn face_for(font: Font) -> Option<&'static FontFile> {
    FONTS
        .iter()
        .find(|face| face.font == font)
        .or_else(|| {
            FONTS
                .iter()
                .find(|face| face.font.family == font.family && face.font.bold == font.bold)
        })
        .or_else(|| {
            FONTS.iter().find(|face| {
                face.font.family == katex_native::Family::Main
                    && face.font.bold == font.bold
                    && face.font.italic == font.italic
            })
        })
        .or_else(|| {
            FONTS
                .iter()
                .find(|face| face.font.family == katex_native::Family::Main)
        })
}

fn family_for(face: &'static FontFile) -> FontFamily {
    FontFamily::Name(face.postscript_name().into())
}

fn install_face(ctx: &egui::Context, face: &'static FontFile) {
    let name = face.postscript_name();
    ctx.add_font(FontInsert::new(
        &name,
        FontData::from_static(face.data),
        vec![InsertFontFamily {
            family: FontFamily::Name(name.clone().into()),
            priority: FontPriority::Highest,
        }],
    ));
}

/// Flatten cubic Béziers and scale every point into points.
fn flatten_path(commands: &[PathCommand], em: f64) -> Vec<Pos2> {
    const STEPS: usize = 10;
    let mut points = Vec::new();
    let mut current = (0.0, 0.0);
    for command in commands {
        match *command {
            PathCommand::MoveTo(x, y) => {
                current = (x, y);
                points.push(Pos2::new((x * em) as f32, (y * em) as f32));
            }
            PathCommand::LineTo(x, y) => {
                current = (x, y);
                points.push(Pos2::new((x * em) as f32, (y * em) as f32));
            }
            PathCommand::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                for step in 1..=STEPS {
                    let t = step as f64 / STEPS as f64;
                    let u = 1.0 - t;
                    let px = u * u * u * current.0
                        + 3.0 * u * u * t * c1x
                        + 3.0 * u * t * t * c2x
                        + t * t * t * x;
                    let py = u * u * u * current.1
                        + 3.0 * u * u * t * c1y
                        + 3.0 * u * t * t * c2y
                        + t * t * t * y;
                    points.push(Pos2::new((px * em) as f32, (py * em) as f32));
                }
                current = (x, y);
            }
            PathCommand::Close => {}
        }
    }
    points
}

/// A convex, non-degenerate polygon in one winding direction.
fn convex(points: &[Pos2]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        let c = points[(index + 2) % points.len()];
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        if cross.abs() <= f32::EPSILON {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

/// The adapter paints without per-item clipping, so a path whose clip would
/// hide part of it is rejected rather than drawn incorrectly.
fn clip_contains(clip: KatexRect, commands: &[PathCommand]) -> bool {
    let mut min = (f64::INFINITY, f64::INFINITY);
    let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut seen = false;
    for command in commands {
        match *command {
            PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => {
                if !x.is_finite() || !y.is_finite() {
                    return false;
                }
                min = (min.0.min(x), min.1.min(y));
                max = (max.0.max(x), max.1.max(y));
                seen = true;
            }
            PathCommand::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                for (x, y) in [(c1x, c1y), (c2x, c2y), (x, y)] {
                    if !x.is_finite() || !y.is_finite() {
                        return false;
                    }
                    min = (min.0.min(x), min.1.min(y));
                    max = (max.0.max(x), max.1.max(y));
                }
                seen = true;
            }
            PathCommand::Close => continue,
        }
    }
    seen && min.0 >= clip.x
        && min.1 >= clip.y
        && max.0 <= clip.x + clip.width
        && max.1 <= clip.y + clip.height
}
