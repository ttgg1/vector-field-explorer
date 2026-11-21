//! Instanced wgpu renderer for arrows, particles and lines (main plan Task 7).
//!
//! Four pipelines (combined 3D scene + flat 2D arrow + line + dot) share one
//! bind-group layout holding the `Globals` uniform and the per-mode instance
//! storage array. The `DrawState` lives in the egui_wgpu callback_resources
//! TypeMap (installed once by the app); each frame a `DrawCallback` uploads the
//! current `SceneGPU` and draws.
//!
//! Depth note: egui_wgpu paint callbacks have no depth attachment, so the 3D
//! combined arrow + particle stream is CPU-sorted far-to-near (painter's
//! algorithm) by the caller. The scene pipeline blends straight alpha
//! (`SrcAlpha` over `OneMinusSrcAlpha`) so plane-by-plane mode can paint the
//! off-plane arrows translucently; selected plane arrows arrive with alpha 1.0
//! and therefore still paint opaquely.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu;
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use glam::{Mat4, Vec3};

use crate::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D, MAX_SIDES, MIN_SIDES};
use crate::color_mapping::{ColorMapping, ColorPalette, ColorQuantity, ColorStats};
use crate::color_ramp::ScaleMode;
use crate::object_renderer::{ObjectPass, ObjectView};
use crate::object_scene::MeshScene;
use crate::scalar_view::ScalarGrid;

/// Native capture failures retain the readback stage and its original cause.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub enum ReadbackError {
    Map(wgpu::BufferAsyncError),
    Poll(wgpu::PollError),
    CallbackLost(std::sync::mpsc::RecvError),
    Range(wgpu::MapRangeError),
    Decode(String),
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for ReadbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Map(e) => write!(f, "readback map failed: {e}"),
            Self::Poll(e) => write!(f, "readback poll failed: {e}"),
            Self::CallbackLost(e) => write!(f, "readback callback receive failed: {e}"),
            Self::Range(e) => write!(f, "readback mapped range unavailable: {e}"),
            Self::Decode(e) => write!(f, "readback RGBA decode failed: {e}"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for ReadbackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Map(e) => Some(e),
            Self::Poll(e) => Some(e),
            Self::CallbackLost(e) => Some(e),
            Self::Range(e) => Some(e),
            Self::Decode(_) => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub enum OffscreenError {
    Setup(String),
    Readback(ReadbackError),
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for OffscreenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Setup(e) => write!(f, "offscreen setup/submission failed: {e}"),
            Self::Readback(e) => e.fmt(f),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for OffscreenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Readback(e) => Some(e),
            Self::Setup(_) => None,
        }
    }
}

/// Classify the real map callback status; also permits deterministic injection.
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub fn readback_map_status(
    status: Result<(), wgpu::BufferAsyncError>,
) -> Result<(), ReadbackError> {
    status.map_err(ReadbackError::Map)
}

/// Per-arrow GPU instance: base position, unit direction + length, color.
///
/// `origin.w` is the kind tag for the ticket-62 combined 3D stream: `1.0` means
/// an arrow, `0.0` means a particle dot (see [`ARROW_KIND`]/[`PARTICLE_KIND`]).
/// Dots keep their radius in `dir_len.w`; arrows keep their length there. Both
/// pipelines share this 48-byte layout and the same storage buffer.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ArrowInstanceGPU {
    pub origin: [f32; 4],  // pos.xyz, kind tag (1.0 arrow / 0.0 particle)
    pub dir_len: [f32; 4], // unit dir.xyz, length (arrows) or radius (dots)
    pub color: [f32; 4],   // rgba 0..1
}

/// Per-line GPU instance: start, end + width, color.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct LineInstanceGPU {
    pub a: [f32; 4],     // start.xyz, 1.0
    pub b: [f32; 4],     // end.xyz, line width
    pub color: [f32; 4], // rgba 0..1
}

/// The largest heatmap texture side accepted by the #92 contract.
///
/// Tied to the scalar grid bound: a heatmap can never upload more rows or
/// columns than the sampler can produce.
pub const MAX_HEATMAP_SIDE: u32 = 160;
const _: () = assert!(MAX_HEATMAP_SIDE == crate::scalar_view::MAX_GRID_SIDE);

/// Vertices one heatmap quad emits: two triangles, six vertices.
pub const HEATMAP_CORNERS: usize = 6;

const HEATMAP_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

/// One textured heatmap quad vertex: world position plus texture coordinate.
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct HeatmapVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// A CPU-built, bounded scalar heatmap for the current plane frame.
///
/// `pixels` is RGBA8 sRGB, one texel per sampled grid point. A texel represents
/// the cell whose lower-left sample is that grid point; cells with any missing
/// corner stay transparent, and the final row/column stay transparent because
/// they begin no cell. Texels are therefore cell-centred while contours and
/// probes sit on sample vertices: the sub-cell offset is intentional and bounded
/// by half a cell, so the fill never claims to align with the overlay lines.
/// `identity` fingerprints the grid and colour mapping so the upload path can
/// skip unchanged frames.
#[derive(Clone, Debug, PartialEq)]
pub struct HeatmapGPU {
    pub size: [u32; 2],
    pub vertices: Vec<HeatmapVertex>,
    pub pixels: Vec<u8>,
    pub identity: u64,
}

/// Legend facts for a heatmap, derived from the same cells the texture paints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatmapLegend {
    pub min: f32,
    pub max: f32,
    pub clipped_low: usize,
    pub clipped_high: usize,
}

impl HeatmapGPU {
    /// Build the bounded heatmap for `grid` under `mapping`.
    ///
    /// Returns `None` when no cell has four finite corners. Colours come from
    /// [`ColorMapping::effective_range`] and the palette's shared
    /// [`crate::color_mapping::ColorPalette::sample_unit`]; out-of-range values
    /// clamp to the ramp endpoint. The quad follows the grid's plane frame, so
    /// the same builder serves every axis.
    ///
    /// The fill and its legend always colour by [`ColorQuantity::Magnitude`].
    /// A cell holds one scalar, not a direction, so a `Direction2d` arrow
    /// mapping must not turn the heatmap into an angle ramp; the arrow path
    /// keeps its own quantity untouched.
    pub fn from_grid(grid: &ScalarGrid, mapping: &ColorMapping) -> Option<Self> {
        let mapping = &heatmap_colour_mapping(mapping);
        let width = grid.width.min(MAX_HEATMAP_SIDE);
        let height = grid.height.min(MAX_HEATMAP_SIDE);
        if width == 0 || height == 0 {
            return None;
        }
        let stats = heatmap_stats(grid);
        if stats.min_magnitude > stats.max_magnitude {
            return None;
        }
        let (min, max) = mapping.effective_range(&stats);
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let mut painted = false;
        for j in 0..height.saturating_sub(1) {
            for i in 0..width.saturating_sub(1) {
                let Some(value) = cell_average(grid, i, j) else {
                    continue;
                };
                painted = true;
                let [red, green, blue] = scalar_colour(value, mapping, min, max);
                let offset = ((j as usize) * (width as usize) + i as usize) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[red, green, blue, 255]);
            }
        }
        if !painted {
            return None;
        }

        let frame = grid.frame;
        // Texture row 0 is the grid's first row and samples at v = 0, so the
        // world-bottom corners use y = 0 and the world-top corners y = 1.
        let corners = [
            (-frame.half_u, -frame.half_v, 0.0, 0.0),
            (frame.half_u, -frame.half_v, 1.0, 0.0),
            (frame.half_u, frame.half_v, 1.0, 1.0),
            (-frame.half_u, -frame.half_v, 0.0, 0.0),
            (frame.half_u, frame.half_v, 1.0, 1.0),
            (-frame.half_u, frame.half_v, 0.0, 1.0),
        ];
        // The resident texture is always MAX_HEATMAP_SIDE square. Scale the
        // quad's normalized coordinates to the uploaded sub-rectangle so a
        // smaller grid does not stretch a mostly-empty 160x160 texture.
        let u_scale = width as f32 / MAX_HEATMAP_SIDE as f32;
        let v_scale = height as f32 / MAX_HEATMAP_SIDE as f32;
        let vertices = corners
            .into_iter()
            .map(|(u, v, x, y)| {
                let position = frame.to_world(Vec3::new(u, v, 0.0));
                HeatmapVertex {
                    position: position.to_array(),
                    uv: [x * u_scale, y * v_scale],
                }
            })
            .collect();

        Some(Self {
            size: [width, height],
            vertices,
            pixels,
            identity: heatmap_identity(grid, mapping),
        })
    }

    fn is_uploadable(&self) -> bool {
        self.vertices.len() == HEATMAP_CORNERS
            && self.size[0] > 0
            && self.size[1] > 0
            && self.size[0] <= MAX_HEATMAP_SIDE
            && self.size[1] <= MAX_HEATMAP_SIDE
            && self.pixels.len() == (self.size[0] as usize) * (self.size[1] as usize) * 4
    }
}

/// Legend facts for the cells [`HeatmapGPU::from_grid`] paints.
pub fn heatmap_legend(grid: &ScalarGrid, mapping: &ColorMapping) -> Option<HeatmapLegend> {
    let mapping = heatmap_colour_mapping(mapping);
    let stats = heatmap_stats(grid);
    if stats.min_magnitude > stats.max_magnitude {
        return None;
    }
    let (min, max) = mapping.effective_range(&stats);
    let mut legend = HeatmapLegend {
        min,
        max,
        clipped_low: 0,
        clipped_high: 0,
    };
    let valid = min.is_finite() && max.is_finite() && min < max;
    for j in 0..grid.height.saturating_sub(1) {
        for i in 0..grid.width.saturating_sub(1) {
            let Some(value) = cell_average(grid, i, j) else {
                continue;
            };
            if !valid {
                continue;
            }
            if value < min {
                legend.clipped_low += 1;
            } else if value > max {
                legend.clipped_high += 1;
            }
        }
    }
    Some(legend)
}

/// Whether `incoming` differs from the resident heatmap identity and needs an
/// upload. A missing incoming heatmap never uploads; the resident resources stay
/// allocated for a later identical identity.
pub fn heatmap_upload_required(resident: Option<u64>, incoming: Option<u64>) -> bool {
    incoming.is_some() && resident != incoming
}

/// Fingerprint the sampled grid and colour mapping for the upload skip seam.
pub fn heatmap_identity(grid: &ScalarGrid, mapping: &ColorMapping) -> u64 {
    let mut hash = DefaultHasher::new();
    for value in [
        grid.frame.origin,
        grid.frame.right,
        grid.frame.up,
        grid.frame.normal,
    ] {
        for component in value.to_array() {
            hash.write_u32(component.to_bits());
        }
    }
    hash.write_u32(grid.frame.half_u.to_bits());
    hash.write_u32(grid.frame.half_v.to_bits());
    hash.write_u32(grid.width);
    hash.write_u32(grid.height);
    hash.write_u64(grid.time.to_bits());
    for sample in &grid.samples {
        match sample.value {
            Some(value) => {
                hash.write_u8(1);
                hash.write_u32(value.to_bits());
            }
            None => hash.write_u8(0),
        }
    }
    match &mapping.palette {
        ColorPalette::Builtin(colormap) => {
            hash.write_u8(0);
            colormap.label().hash(&mut hash);
        }
        ColorPalette::Custom(ramp) => {
            hash.write_u8(1);
            hash.write_u8(ramp.reversed as u8);
            ramp.kind.label().hash(&mut hash);
            for stop in &ramp.stops {
                hash.write_u32(stop.position.to_bits());
                hash.write(&stop.color.0);
            }
        }
    }
    match mapping.mode {
        ScaleMode::Linear => hash.write_u8(0),
        ScaleMode::SignedLog { half_width } => {
            hash.write_u8(1);
            hash.write_u32(half_width.to_bits());
        }
    }
    mapping.quantity.label().hash(&mut hash);
    match mapping.manual_range {
        Some((min, max)) => {
            hash.write_u8(1);
            hash.write_u32(min.to_bits());
            hash.write_u32(max.to_bits());
        }
        None => hash.write_u8(0),
    }
    hash.finish()
}

fn heatmap_stats(grid: &ScalarGrid) -> ColorStats {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut max_abs: f32 = 0.0;
    for j in 0..grid.height.saturating_sub(1) {
        for i in 0..grid.width.saturating_sub(1) {
            let Some(value) = cell_average(grid, i, j) else {
                continue;
            };
            min = min.min(value);
            max = max.max(value);
            max_abs = max_abs.max(value.abs());
        }
    }
    if min > max {
        ColorStats::default()
    } else {
        ColorStats {
            min_magnitude: min,
            max_magnitude: max,
            max_abs_component: max_abs,
        }
    }
}

/// Cell colour from the mean of its four finite corners. A cell with any
/// missing corner returns `None` and is left transparent.
fn cell_average(grid: &ScalarGrid, i: u32, j: u32) -> Option<f32> {
    if !grid.cell_is_painted(i, j) {
        return None;
    }
    let values = [
        grid.sample(i, j).value?,
        grid.sample(i + 1, j).value?,
        grid.sample(i, j + 1).value?,
        grid.sample(i + 1, j + 1).value?,
    ];
    Some(values.iter().sum::<f32>() / 4.0)
}

/// The heatmap's own colour mapping: the active palette and mode, but always
/// magnitude-based. `Direction2d` describes arrows, not a filled scalar cell.
fn heatmap_colour_mapping(mapping: &ColorMapping) -> ColorMapping {
    ColorMapping {
        quantity: ColorQuantity::Magnitude,
        ..mapping.clone()
    }
}

fn scalar_colour(value: f32, mapping: &ColorMapping, min: f32, max: f32) -> [u8; 3] {
    if !value.is_finite() {
        return [128, 128, 128];
    }
    // Signed log falls back to linear exactly when the mapping's effective
    // mode does; the shared ScaleMode normalizer supplies the fallback.
    let Some(t) = mapping
        .mode
        .normalize(value, min, max)
        .or_else(|| ScaleMode::Linear.normalize(value, min, max))
    else {
        return [128, 128, 128];
    };
    mapping.palette.sample_unit(t)
}

/// Shared per-frame uniforms.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
    pub light_dir: [f32; 4],
    /// World-space camera right axis, w = 0. Dot billboards expand along it.
    pub camera_right: [f32; 4],
    /// World-space camera up axis, w = 0.
    pub camera_up: [f32; 4],
    /// The active view's style id (3D family in 3D, 2D family in 2D), radial
    /// side count, then two unused slots.
    pub arrow_style: [u32; 4],
    /// Head length, head width, shaft radius, then one unused slot.
    pub arrow_params: [f32; 4],
}

/// Vertices the 3D `Flat` style emits per instance: a shaft quad (6) and a head
/// triangle (3). Kept in step with `ARROW_VERTS_FLAT` in the arrow shader.
pub const ARROW_VERTS_FLAT: u32 = 9;

/// Vertices the 3D `Line` style emits per instance: one headless ribbon quad.
/// Kept in step with `ARROW_VERTS_LINE` in the arrow shader.
pub const ARROW_VERTS_LINE: u32 = 6;

/// Vertices the flat 2D `Flat` arrow emits per instance. Kept in step with
/// `ARROW2D_VERTS` in `src/shaders.arrow2d.wgsl`.
pub const ARROW2D_VERTS: u32 = 9;

/// Vertices the 2D `Line` arrow emits per instance: one headless quad. Kept in
/// step with `ARROW2D_VERTS_LINE` in `src/shaders.arrow2d.wgsl`.
pub const ARROW2D_VERTS_LINE: u32 = 6;

/// The per-instance vertex budget for the ticket-62 combined 3D draw. One draw
/// call cannot vary its vertex count per instance, so every instance (arrow or
/// particle) gets the widest arrow family's budget: the cone at [`MAX_SIDES`]
/// sides, `12 * sides - 6`. The merged shader derives the instance index from
/// this budget and exits unused invocations before any ring/trig work.
pub const MAX_ARROW_VERTS: u32 = 12 * MAX_SIDES - 6;

/// `origin.w` tag for an arrow instance in the combined 3D stream.
pub const ARROW_KIND: f32 = 1.0;

/// `origin.w` tag for a particle (dot) instance in the combined 3D stream.
pub const PARTICLE_KIND: f32 = 0.0;

/// Which arrow pipeline `record` uses and how many vertices per instance it
/// draws. Pure so the 2D/3D selection is testable without a GPU; `record`
/// matches on it directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDrawPlan {
    /// The flat 2D arrow through `arrow2d_pipeline`, at the selected 2D
    /// family's vertex count. 2D keeps its arrows-then-dots path and never
    /// reads the combined stream.
    Flat2D { vertices: u32 },
    /// The ticket-62 combined arrow + particle stream through `scene_pipeline`,
    /// at the fixed [`MAX_ARROW_VERTS`] per-instance budget.
    Combined3D { budget: u32 },
}

/// One draw the renderer records, in painter order. Structural only: the
/// tests use it to prove 3D is lines → one combined draw and 2D keeps its
/// arrows-then-dots draws, without a GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawPass {
    /// Filled scalar heatmap quad, six vertices, painted before every other
    /// collection so contours and arrows stay readable.
    Heatmap { vertices: usize },
    /// Backdrop/streamline lines, six vertices per instance.
    Lines { instances: usize },
    /// Fading particle trail segments, six vertices per instance. Painted in
    /// both views behind the arrows and particles so trails read as motion
    /// history rather than as foreground geometry (ticket 48).
    Trails { instances: usize },
    /// The single 3D combined arrow + particle draw.
    Scene3D { instances: usize, budget: u32 },
    /// The flat 2D arrow draw at its family's vertex count.
    Arrow2D { instances: usize, vertices: u32 },
    /// The 2D dot billboard draw (six vertices per instance).
    Dots { instances: usize },
}

/// One painter step. [`DrawStage::Objects`] is the composite of the offscreen
/// object image; every later stage is an existing vector draw. `record` walks
/// [`paint_stages`], so the ordering is observable without a GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawStage {
    /// Composite the prepared object target before any vector geometry.
    Objects,
    /// One of the existing arrow/line/dot draws.
    Vector(DrawPass),
}

/// Device-free state of the object scene owned by [`DrawState`].
///
/// `DrawState` needs a wgpu device to build its pipelines, so this slot is the
/// test seam for "starts empty", "keeps the scene's generation" and "an empty
/// scene draws nothing". It allocates only the `Arc` it is handed.
#[derive(Clone, Default)]
pub struct ObjectSceneState {
    scene: Option<Arc<MeshScene>>,
}

impl ObjectSceneState {
    /// An empty slot: no object scene has been installed for this frame.
    #[must_use]
    pub fn new() -> Self {
        Self { scene: None }
    }

    /// The slot holding an already-built scene.
    #[must_use]
    pub fn from_scene(scene: Arc<MeshScene>) -> Self {
        Self { scene: Some(scene) }
    }

    /// Install this frame's scene; the `Arc` and its generation are unchanged.
    pub fn set(&mut self, scene: Arc<MeshScene>) {
        self.scene = Some(scene);
    }

    /// Clear the slot when a capture or frame has no object scene.
    pub fn clear(&mut self) {
        self.scene = None;
    }

    /// The installed scene, if any.
    #[must_use]
    pub fn get(&self) -> Option<&Arc<MeshScene>> {
        self.scene.as_ref()
    }

    /// Objects in the installed scene; zero when nothing is installed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.scene.as_ref().map_or(0, |scene| scene.objects().len())
    }

    /// Whether the slot is empty (no scene or a scene with no mapped bodies).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether the installed scene has at least one mapped body.
    #[must_use]
    pub fn has_objects(&self) -> bool {
        !self.is_empty()
    }
}

/// The painter order for one frame: the object composite first (only when the
/// scene has at least one mapped body), then the unchanged vector passes.
///
/// The returned `Vec` is one small allocation per paint (at most seven stages,
/// with no per-instance growth). The draw pipeline is defined by this ordered
/// slice in both the live callback and the offscreen capture; that bounded
/// allocation is accepted rather than threading a bespoke iterator/visitor
/// through both paths.
#[must_use]
pub fn paint_stages(scene: &SceneGPU, object_count: usize) -> Vec<DrawStage> {
    let mut stages = Vec::with_capacity(7);
    if object_count > 0 {
        stages.push(DrawStage::Objects);
    }
    stages.extend(scene.draw_passes().into_iter().map(DrawStage::Vector));
    stages
}

/// Physical-pixel extent of the egui callback viewport, rounded like
/// `egui::PaintCallbackInfo::viewport_in_pixels`. The screen-edge clamp is left
/// to the host: the target only needs a nonzero extent.
#[must_use]
pub fn object_viewport_pixels(viewport: egui::Rect, pixels_per_point: f32) -> [u32; 2] {
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return [0, 0];
    }
    let left = (pixels_per_point * viewport.min.x).round();
    let top = (pixels_per_point * viewport.min.y).round();
    let right = (pixels_per_point * viewport.max.x).round();
    let bottom = (pixels_per_point * viewport.max.y).round();
    if !left.is_finite() || !top.is_finite() || !right.is_finite() || !bottom.is_finite() {
        return [0, 0];
    }
    let width = (right - left).max(0.0);
    let height = (bottom - top).max(0.0);
    [width as u32, height as u32]
}

/// Vertices the dot shader emits per instance: the two triangles of a
/// camera-facing billboard quad. Kept in step with
/// [`crate::particles::DOT_QUAD_CORNERS`] (the CPU mirror of the same table).
pub const DOT_VERTS_PER_INSTANCE: u32 = 6;

/// The world-space camera right/up axes a billboard needs, recovered from the
/// view-projection's first two rows.
///
/// Rows 0/1 of a standard perspective or orthographic projection are scaled
/// copies of the view matrix's right/up rows (both projections are diagonal in
/// x and y), so normalising them gives the camera basis for either projection
/// mode. That matters: the orbit camera's own `view_basis()` only describes the
/// 3D orbit, while 2D mode paints through the front view.
pub fn camera_basis_from_view_proj(view_proj: &Mat4) -> (Vec3, Vec3) {
    let right = view_proj.row(0).truncate().normalize_or_zero();
    let up = view_proj.row(1).truncate().normalize_or_zero();
    (right, up)
}

/// The GPU scene uploaded each frame. `Arc` lets the app skip re-uploads when
/// a collection is unchanged (dots reuse the arrow instance layout with a
/// negligible head).
///
/// The arrow fields select the geometry family and the 2D/3D draw path. They
/// travel with the instance collections so screenshots and the viewport paint
/// exactly the same scene.
///
/// `combined` is the ticket-62 3D stream: arrows followed by dots, sorted
/// far-to-near by the app each frame because the egui_wgpu pass has no depth
/// attachment. `arrows`/`dots` stay the frame-invariant sources (and drive the
/// 2D path); 3D `record` reads only `combined`.
#[derive(Clone)]
pub struct SceneGPU {
    pub arrows: Arc<Vec<ArrowInstanceGPU>>,
    pub lines: Arc<Vec<LineInstanceGPU>>,
    /// Fading particle trail segments (ticket 48), painted between the
    /// backdrop lines and the combined 3D arrow + particle draw.
    pub trails: Arc<Vec<LineInstanceGPU>>,
    pub dots: Arc<Vec<ArrowInstanceGPU>>,
    pub combined: Arc<Vec<ArrowInstanceGPU>>,
    /// Filled scalar heatmap (issue #92 slice B2). `None` keeps every existing
    /// scene byte-identical: no draw, no upload and no painter-order change.
    pub heatmap: Option<Arc<HeatmapGPU>>,
    pub arrow_style: ArrowStyle,
    pub arrow_style_2d: ArrowStyle2D,
    pub arrow_params: ArrowParams,
    pub mode_3d: bool,
}

impl Default for SceneGPU {
    fn default() -> Self {
        Self {
            arrows: Arc::new(Vec::new()),
            lines: Arc::new(Vec::new()),
            trails: Arc::new(Vec::new()),
            dots: Arc::new(Vec::new()),
            combined: Arc::new(Vec::new()),
            heatmap: None,
            arrow_style: ArrowStyle::default(),
            arrow_style_2d: ArrowStyle2D::default(),
            arrow_params: ArrowParams::default(),
            // The old renderer always drew the perspective mesh; keep that as
            // the default until the viewmodel's snapshot fills in the mode.
            mode_3d: true,
        }
    }
}

impl SceneGPU {
    pub fn is_empty(&self) -> bool {
        self.arrows.is_empty()
            && self.lines.is_empty()
            && self.trails.is_empty()
            && self.dots.is_empty()
            && self.combined.is_empty()
            && self.heatmap.is_none()
    }

    /// The pipeline and per-instance vertex count the next `record` call uses.
    /// 3D uses one combined stream at the fixed [`MAX_ARROW_VERTS`] budget; 2D
    /// uses the independent [`ArrowStyle2D`] set (tickets 61 and 64), never the
    /// 3D selection, and carries that 2D family's own 9- or 6-vertex count.
    pub fn arrow_draw_plan(&self) -> ArrowDrawPlan {
        if self.mode_3d {
            ArrowDrawPlan::Combined3D {
                budget: MAX_ARROW_VERTS,
            }
        } else {
            ArrowDrawPlan::Flat2D {
                vertices: self.arrow_style_2d.vertex_count(self.arrow_params.sides),
            }
        }
    }

    /// The per-instance vertex count the next `record` call draws: the combined
    /// 3D budget, or the selected 2D family's own count.
    pub fn arrow_vertex_count(&self) -> u32 {
        match self.arrow_draw_plan() {
            ArrowDrawPlan::Flat2D { vertices } => vertices,
            ArrowDrawPlan::Combined3D { budget } => budget,
        }
    }

    /// The current family's real geometry count, independent of the combined
    /// draw's budget. Keeps the per-family vertex-count coverage that used to
    /// live on [`arrow_vertex_count`] (ticket 62 moved it here).
    pub fn style_vertex_count(&self) -> u32 {
        if self.mode_3d {
            self.arrow_style.vertex_count(self.arrow_params.sides)
        } else {
            self.arrow_style_2d.vertex_count(self.arrow_params.sides)
        }
    }

    /// Whether arrows paint through the flat 2D pipeline rather than the 3D
    /// combined pipeline.
    pub fn uses_flat_arrow(&self) -> bool {
        matches!(self.arrow_draw_plan(), ArrowDrawPlan::Flat2D { .. })
    }

    /// The draws `record` performs, in painter order. In both views the
    /// backdrop lines and fading particle trails come first. 3D then issues
    /// exactly one combined arrow+particle draw (even when the arrow list is
    /// empty: the dots live in `combined`); 2D keeps its arrows-then-dots draws
    /// and ignores `combined`. A 2D plane slice can carry trails; ordinary
    /// two-component 2D simply has none.
    pub fn draw_passes(&self) -> Vec<DrawPass> {
        let mut passes = Vec::with_capacity(6);
        if let Some(heatmap) = self.heatmap.as_deref()
            && heatmap.vertices.len() == HEATMAP_CORNERS
        {
            passes.push(DrawPass::Heatmap {
                vertices: HEATMAP_CORNERS,
            });
        }
        if !self.lines.is_empty() {
            passes.push(DrawPass::Lines {
                instances: self.lines.len(),
            });
        }
        if !self.trails.is_empty() {
            passes.push(DrawPass::Trails {
                instances: self.trails.len(),
            });
        }
        if self.mode_3d {
            if !self.combined.is_empty() {
                passes.push(DrawPass::Scene3D {
                    instances: self.combined.len(),
                    budget: MAX_ARROW_VERTS,
                });
            }
        } else {
            if !self.arrows.is_empty() {
                passes.push(DrawPass::Arrow2D {
                    instances: self.arrows.len(),
                    // The 2D branch of `style_vertex_count` is the selected
                    // family's real geometry count; using the accessor keeps
                    // one source for it (production caller, ticket 62 fix).
                    vertices: self.style_vertex_count(),
                });
            }
            if !self.dots.is_empty() {
                passes.push(DrawPass::Dots {
                    instances: self.dots.len(),
                });
            }
        }
        passes
    }
}

/// Vertices one combined 3D draw issues for `instances` instances: every
/// instance consumes the fixed [`MAX_ARROW_VERTS`] budget.
pub fn scene_vertex_count(instances: usize) -> u32 {
    (instances as u32).saturating_mul(MAX_ARROW_VERTS)
}

/// Build the ticket-62 combined 3D stream: arrows then dots, with `origin.w`
/// forced to [`ARROW_KIND`]/[`PARTICLE_KIND`] so the shader can branch on the
/// kind. Order is the frame-invariant input order; [`sort_scene_back_to_front`]
/// then sorts the whole stream. Equal-depth mixed kinds therefore keep this
/// concat order (arrows before dots).
pub fn combined_instances(
    arrows: &[ArrowInstanceGPU],
    dots: &[ArrowInstanceGPU],
) -> Vec<ArrowInstanceGPU> {
    let mut combined = Vec::with_capacity(arrows.len() + dots.len());
    combined.extend(arrows.iter().copied().map(|mut arrow| {
        arrow.origin[3] = ARROW_KIND;
        arrow
    }));
    combined.extend(dots.iter().copied().map(|mut dot| {
        dot.origin[3] = PARTICLE_KIND;
        dot
    }));
    combined
}

pub(crate) const LIGHT_DIR: [f32; 4] = [0.4211, 0.8521, 0.3108, 0.0]; // normalize(0.42, 0.85, 0.31)

/// The arrow pipeline's blend state: straight alpha, so an instance's
/// `color.a` is honoured. Plane-by-plane mode relies on this; opaque arrows
/// (alpha 1.0) are unchanged by it.
///
/// There is no depth attachment to configure: egui_wgpu paint callbacks run
/// in a pass egui owns, so the existing far-to-near CPU sort is the depth
/// ordering. A second opaque pass for the selected plane is unnecessary:
/// alpha 1.0 under this blend resolves to a plain replace, so selected arrows
/// still paint opaquely inside the one sorted draw.
pub const ARROW_BLEND: wgpu::BlendState = wgpu::BlendState::ALPHA_BLENDING;

/// The flat 2D arrow pipeline shares the arrow blend: off-plane dimming and
/// screenshots must behave identically in both views.
pub const ARROW2D_BLEND: wgpu::BlendState = ARROW_BLEND;

/// The line pipeline's blend state (ticket 48): straight alpha like the arrows,
/// so trail segments can fade toward their tail. Streamlines, the grid and the
/// axes all carry alpha 1.0, so their image is unchanged.
pub const LINE_BLEND: wgpu::BlendState = ARROW_BLEND;

/// The dot pipeline's blend state: straight alpha, so particle opacity works
/// and the billboard rim keeps its antialiasing.
pub const DOT_BLEND: wgpu::BlendState = wgpu::BlendState::ALPHA_BLENDING;

fn mat4_to_mats(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

/// One-time GPU resources: pipelines, layouts, and the (per-frame rewritten)
/// buffers. Installed into the egui_wgpu callback_resources TypeMap.
pub struct DrawState {
    bind_layout: wgpu::BindGroupLayout,
    scene_pipeline: wgpu::RenderPipeline,
    arrow2d_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    dot_pipeline: wgpu::RenderPipeline,
    heatmap_pipeline: wgpu::RenderPipeline,
    globals_buf: wgpu::Buffer,
    arrow_buf: wgpu::Buffer,
    line_buf: wgpu::Buffer,
    trail_buf: wgpu::Buffer,
    dot_buf: wgpu::Buffer,
    arrow_bind: wgpu::BindGroup,
    line_bind: wgpu::BindGroup,
    trail_bind: wgpu::BindGroup,
    dot_bind: wgpu::BindGroup,
    heatmap_bind: wgpu::BindGroup,
    heatmap_vertex_buf: wgpu::Buffer,
    /// Kept alive for the bind group; the texture is allocated once and reused.
    _heatmap_texture: wgpu::Texture,
    heatmap_identity: Option<u64>,
    light_dir: [f32; 4],
    /// The offscreen object pass. It allocates no target until a non-empty
    /// scene with a nonzero viewport arrives.
    object_pass: ObjectPass,
    /// This frame's mapped object scene, installed by the draw callback.
    object_scene: ObjectSceneState,
    /// The surface format the pipelines were built for. An offscreen capture
    /// renders in the same format, so the image matches the screen and the
    /// readback only needs the channel-order fix-up.
    format: wgpu::TextureFormat,
}

impl DrawState {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader_arrow = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe arrow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.arrow.wgsl").into()),
        });
        let shader_arrow2d = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe 2d arrow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.arrow2d.wgsl").into()),
        });
        let shader_line = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe line shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.line.wgsl").into()),
        });
        let shader_dot = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe dot shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.dot.wgsl").into()),
        });
        let shader_heatmap = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe heatmap shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.heatmap.wgsl").into()),
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vfe globals+instances bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let heatmap_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vfe heatmap globals+texture bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vfe pipeline layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let heatmap_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vfe heatmap pipeline layout"),
                bind_group_layouts: &[Some(&heatmap_bind_layout)],
                immediate_size: 0,
            });

        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe scene pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_arrow,
                entry_point: Some("vs_scene"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_arrow,
                entry_point: Some("fs_scene"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(ARROW_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let arrow2d_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe 2d arrow pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_arrow2d,
                entry_point: Some("vs_arrow2d"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_arrow2d,
                entry_point: Some("fs_arrow2d"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(ARROW2D_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe line pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_line,
                entry_point: Some("vs_line"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_line,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(LINE_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let dot_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe dot pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_dot,
                entry_point: Some("vs_dot"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_dot,
                entry_point: Some("fs_dot"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // The rim is antialiased by fading the fragment's alpha
                    // over one pixel, so the dot pipeline blends; fully covered
                    // interior pixels (alpha 1) are indistinguishable from the
                    // arrows' REPLACE blend. Particle opacity (ticket 48) rides
                    // the instance alpha through this same state.
                    blend: Some(DOT_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let heatmap_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe heatmap pipeline"),
            layout: Some(&heatmap_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_heatmap,
                entry_point: Some("vs_heatmap"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<HeatmapVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &HEATMAP_VERTEX_ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_heatmap,
                entry_point: Some("fs_heatmap"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Placeholder buffers (zero-length upload is illegal; start with one
        // instance each and let the first prepare() resize them).
        let globals_buf = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("vfe globals"),
                contents: bytemuck::bytes_of(&Globals {
                    view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                    inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                    camera_pos: [0.0; 4],
                    light_dir: LIGHT_DIR,
                    camera_right: [1.0, 0.0, 0.0, 0.0],
                    camera_up: [0.0, 1.0, 0.0, 0.0],
                    arrow_style: [ArrowStyle::Cone.id(), ArrowParams::default().sides, 0, 0],
                    arrow_params: [0.3, 0.12, 0.055, 0.0],
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );
        let arrow_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe arrows"),
            size: std::mem::size_of::<ArrowInstanceGPU>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let line_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe lines"),
            size: std::mem::size_of::<LineInstanceGPU>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let trail_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe trails"),
            size: std::mem::size_of::<LineInstanceGPU>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dot_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe dots"),
            size: std::mem::size_of::<ArrowInstanceGPU>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let arrow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vfe arrow bind"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: arrow_buf.as_entire_binding(),
                },
            ],
        });
        let line_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vfe line bind"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: line_buf.as_entire_binding(),
                },
            ],
        });
        let trail_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vfe trail bind"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: trail_buf.as_entire_binding(),
                },
            ],
        });
        let dot_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vfe dot bind"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dot_buf.as_entire_binding(),
                },
            ],
        });

        // Allocated once at the contract's maximum side; each upload writes only
        // the current heatmap's sub-rectangle and the resident identity skips
        // unchanged frames entirely.
        let heatmap_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vfe heatmap texture"),
            size: wgpu::Extent3d {
                width: MAX_HEATMAP_SIDE,
                height: MAX_HEATMAP_SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let heatmap_view = heatmap_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let heatmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vfe heatmap sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let heatmap_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vfe heatmap bind"),
            layout: &heatmap_bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&heatmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&heatmap_sampler),
                },
            ],
        });
        let heatmap_vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe heatmap vertices"),
            size: (HEATMAP_CORNERS * std::mem::size_of::<HeatmapVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            bind_layout,
            scene_pipeline,
            arrow2d_pipeline,
            line_pipeline,
            dot_pipeline,
            heatmap_pipeline,
            globals_buf,
            arrow_buf,
            line_buf,
            trail_buf,
            dot_buf,
            arrow_bind,
            line_bind,
            trail_bind,
            dot_bind,
            heatmap_bind,
            heatmap_vertex_buf,
            _heatmap_texture: heatmap_texture,
            heatmap_identity: None,
            light_dir: LIGHT_DIR,
            object_pass: ObjectPass::new(device, format),
            object_scene: ObjectSceneState::new(),
            format,
        }
    }

    /// Install the object scene this frame composites. The scene is carried by
    /// `Arc`; its content fingerprint is never rewritten here.
    pub fn set_object_scene(&mut self, scene: Arc<MeshScene>) {
        self.object_scene.set(scene);
    }

    /// The object scene installed for the current frame, if any.
    #[must_use]
    pub fn object_scene(&self) -> Option<&Arc<MeshScene>> {
        self.object_scene.get()
    }

    /// The resident heatmap identity, exposed for the upload-skip seam tests.
    #[doc(hidden)]
    pub fn resident_heatmap_identity(&self) -> Option<u64> {
        self.heatmap_identity
    }

    /// Rebuild instance buffers + write uniforms for this frame's scene.
    /// Takes the whole [`SceneGPU`]: its instance slices are the payload, and
    /// bundling them keeps the argument list clippy-clean.
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        camera_pos: Vec3,
        scene: &SceneGPU,
    ) {
        let (camera_right, camera_up) = camera_basis_from_view_proj(&view_proj);
        let params = scene.arrow_params.sanitized();
        // `sanitized` is the single 4..=16 side clamp; re-clamp explicitly and
        // pin it so the uniform can never carry a raw out-of-range side count
        // (the shader only guards the low end).
        let sides = params.sides.clamp(MIN_SIDES, MAX_SIDES);
        debug_assert!(
            (MIN_SIDES..=MAX_SIDES).contains(&params.sides),
            "sanitized sides stay within {MIN_SIDES}..={MAX_SIDES}",
        );
        // `arrow_style.x` is mode-dependent: the 3D family id while the 3D
        // pipeline paints, the 2D family id while the 2D pipeline paints. The
        // 2D and 3D ids are separate namespaces, so the matching shader must
        // be the one that reads it (ticket 64).
        let arrow_style_id = if scene.mode_3d {
            scene.arrow_style.id()
        } else {
            scene.arrow_style_2d.id()
        };
        let globals = Globals {
            view_proj: mat4_to_mats(view_proj),
            inv_view_proj: mat4_to_mats(view_proj.inverse()),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 1.0],
            light_dir: self.light_dir,
            camera_right: camera_right.extend(0.0).to_array(),
            camera_up: camera_up.extend(0.0).to_array(),
            arrow_style: [arrow_style_id, sides, 0, 0],
            arrow_params: [
                params.head_length,
                params.head_width,
                params.shaft_radius,
                0.0,
            ],
        };
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));

        // Empty collections are skipped: a zero-size storage buffer cannot be
        // bound (the bind group would be rejected with "binding size is zero").
        // The one-instance placeholders from `new()` remain bound and valid,
        // and `record` skips drawing any empty collection anyway.
        //
        // 3D binds the combined sorted stream (arrows + dots) to the arrow
        // slot, so an all-particle scene still binds a buffer. 2D binds its
        // arrow list instead and never reads `combined`.
        let arrow_instances: &[ArrowInstanceGPU] = if scene.mode_3d {
            scene.combined.as_slice()
        } else {
            scene.arrows.as_slice()
        };
        if !arrow_instances.is_empty() {
            self.arrow_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe scene instances (frame)"),
                contents: bytemuck::cast_slice(arrow_instances),
                usage: wgpu::BufferUsages::STORAGE,
            });
            self.arrow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vfe scene bind (frame)"),
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.arrow_buf.as_entire_binding(),
                    },
                ],
            });
        }

        if !scene.lines.is_empty() {
            self.line_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe lines (frame)"),
                contents: bytemuck::cast_slice(scene.lines.as_slice()),
                usage: wgpu::BufferUsages::STORAGE,
            });
            self.line_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vfe line bind (frame)"),
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.line_buf.as_entire_binding(),
                    },
                ],
            });
        }

        // Trails have their own buffer: streamlines upload only on rebuild,
        // trails every frame, so sharing `line_buf` would re-upload the whole
        // backdrop each frame.
        if !scene.trails.is_empty() {
            self.trail_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe trails (frame)"),
                contents: bytemuck::cast_slice(scene.trails.as_slice()),
                usage: wgpu::BufferUsages::STORAGE,
            });
            self.trail_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vfe trail bind (frame)"),
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.trail_buf.as_entire_binding(),
                    },
                ],
            });
        }

        // The dot buffer feeds the 2D path only; 3D dots travel in `combined`.
        if !scene.mode_3d && !scene.dots.is_empty() {
            self.dot_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe dots (frame)"),
                contents: bytemuck::cast_slice(scene.dots.as_slice()),
                usage: wgpu::BufferUsages::STORAGE,
            });
            self.dot_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vfe dot bind (frame)"),
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.dot_buf.as_entire_binding(),
                    },
                ],
            });
        }

        // The heatmap texture and its six-vertex quad are written only when the
        // grid/mapping identity changes. A scene without a heatmap touches
        // neither path.
        if let Some(heatmap) = scene.heatmap.as_deref()
            && heatmap.is_uploadable()
            && heatmap_upload_required(self.heatmap_identity, Some(heatmap.identity))
        {
            // These rows are unpadded on purpose: `write_texture` copies straight
            // from host memory and only buffer-copy layouts (such as the
            // readback path) need `COPY_BYTES_PER_ROW_ALIGNMENT` (256 bytes).
            // The heatmap pipeline targets the surface's blendable format with
            // an `Rgba8UnormSrgb` texture; if a future target format is not
            // blendable, disable the pass with a reason instead of degrading it.
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self._heatmap_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &heatmap.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(heatmap.size[0] * 4),
                    rows_per_image: Some(heatmap.size[1]),
                },
                wgpu::Extent3d {
                    width: heatmap.size[0],
                    height: heatmap.size[1],
                    depth_or_array_layers: 1,
                },
            );
            queue.write_buffer(
                &self.heatmap_vertex_buf,
                0,
                bytemuck::cast_slice(&heatmap.vertices),
            );
            self.heatmap_identity = Some(heatmap.identity);
        }
    }

    /// Record the object composite then the vector passes. `object_count` is
    /// the number of mapped bodies in the prepared scene; it only decides
    /// whether the composite stage exists. `composite_guard_rect` bounds the
    /// composite: the live callback passes the callback rect in egui points,
    /// while the offscreen capture passes a pixel-sized rect
    /// (`0..width`, `0..height`). `ObjectPass::paint` only needs a positive,
    /// finite extent, so either unit works; callers must not mix them.
    fn record_with_objects(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        scene: &SceneGPU,
        object_count: usize,
        composite_guard_rect: egui::Rect,
    ) {
        for stage in paint_stages(scene, object_count) {
            match stage {
                DrawStage::Objects => self.object_pass.paint(pass, composite_guard_rect),
                DrawStage::Vector(draw) => self.record_draw(pass, draw),
            }
        }
    }

    fn record_draw(&self, pass: &mut wgpu::RenderPass<'_>, draw: DrawPass) {
        match draw {
            DrawPass::Heatmap { vertices } => {
                pass.set_pipeline(&self.heatmap_pipeline);
                pass.set_bind_group(0, &self.heatmap_bind, &[]);
                pass.set_vertex_buffer(0, self.heatmap_vertex_buf.slice(..));
                pass.draw(0..vertices as u32, 0..1);
            }
            DrawPass::Lines { instances } => {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_bind_group(0, &self.line_bind, &[]);
                pass.draw(0..(instances as u32) * 6, 0..1);
            }
            DrawPass::Trails { instances } => {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_bind_group(0, &self.trail_bind, &[]);
                pass.draw(0..(instances as u32) * 6, 0..1);
            }
            DrawPass::Scene3D { instances, .. } => {
                pass.set_pipeline(&self.scene_pipeline);
                pass.set_bind_group(0, &self.arrow_bind, &[]);
                pass.draw(0..scene_vertex_count(instances), 0..1);
            }
            DrawPass::Arrow2D {
                instances,
                vertices,
            } => {
                pass.set_pipeline(&self.arrow2d_pipeline);
                pass.set_bind_group(0, &self.arrow_bind, &[]);
                pass.draw(0..(instances as u32) * vertices, 0..1);
            }
            DrawPass::Dots { instances } => {
                pass.set_pipeline(&self.dot_pipeline);
                pass.set_bind_group(0, &self.dot_bind, &[]);
                pass.draw(0..(instances as u32) * DOT_VERTS_PER_INSTANCE, 0..1);
            }
        }
    }

    /// Submit `scene` into a fresh offscreen texture at `width`×`height` and
    /// return the readback buffer. The caller maps it: native waits, wasm
    /// resolves the map promise on the browser event loop.
    ///
    /// The caller passes the same `view_proj`/`camera_pos` the viewport is
    /// painting with, so the capture is the current view rather than a
    /// re-derived one. The texture borrows the surface format (the pipelines
    /// were created for it) and lives only for this submission: the live
    /// surface textures are never touched. `clear` is the viewport background
    /// in linear space, matching what egui paints behind the field.
    #[allow(clippy::too_many_arguments)]
    fn submit_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneGPU,
        objects: Option<&Arc<MeshScene>>,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<wgpu::Buffer, String> {
        // Reject formats the PNG path cannot describe up front: a readback
        // stride/buffer sized for RGBA8 is wrong for, say, a 16-float band and
        // would surface as a wgpu validation panic instead of this error.
        crate::screenshot::png_readback_check(self.format)?;

        // Same uniforms + instance buffers the on-screen pass would use. The
        // paint callback re-uploads every frame, so this does not leave the
        // live frame stale.
        self.upload(device, queue, view_proj, camera_pos, scene);

        // One encoder carries the whole capture in submission order: the object
        // image is recorded first, the screenshot pass composites it before any
        // vector draw, and the readback copy follows. wgpu inserts the usage
        // transition between the object target write and the later sample.
        // An empty scene allocates nothing and draws nothing.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vfe screenshot encoder"),
        });
        if let Some(objects) = objects {
            self.set_object_scene(Arc::clone(objects));
            let object_view = ObjectView {
                view_proj: view_proj.to_cols_array_2d(),
                eye: camera_pos.to_array(),
                viewport: [width, height],
                wireframe: false,
                clear,
            };
            self.object_pass
                .prepare(device, queue, &mut encoder, objects, &object_view)?;
        } else {
            self.object_scene.clear();
        }

        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vfe screenshot target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

        let padded_row = crate::screenshot::padded_row_bytes(width);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe screenshot readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vfe screenshot pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let composite_guard_rect = egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width as f32, height as f32),
            );
            let object_count = objects.map_or(0, |scene| scene.objects().len());
            self.record_with_objects(&mut pass, scene, object_count, composite_guard_rect);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        Ok(readback)
    }

    /// Render `scene` offscreen and read it back synchronously (native).
    ///
    /// `map_async` is followed by `poll(Wait)` so the mapping cannot race the
    /// copy; one 4k frame is a short stall on native, where the save dialog
    /// runs on a worker thread afterwards.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneGPU,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<Vec<u8>, OffscreenError> {
        self.render_offscreen_with_objects(
            device, queue, scene, None, view_proj, camera_pos, width, height, clear,
        )
    }

    /// As [`Self::render_offscreen`], compositing the object image first when
    /// `objects` is a non-empty scene. The live viewport and the GUI screenshot
    /// capture use this. CLI PNG and CLI animation exports still call the
    /// no-objects form, so they do not yet composite objects (issue #192).
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub fn render_offscreen_with_objects(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneGPU,
        objects: Option<&Arc<MeshScene>>,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<Vec<u8>, OffscreenError> {
        let readback = self
            .submit_offscreen(
                device, queue, scene, objects, view_proj, camera_pos, width, height, clear,
            )
            .map_err(OffscreenError::Setup)?;

        let slice = readback.slice(..);
        let (mapped_tx, mapped_rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = mapped_tx.send(result);
        });
        let result = (|| {
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(ReadbackError::Poll)?;
            readback_map_status(mapped_rx.recv().map_err(ReadbackError::CallbackLost)?)?;

            let pixels = {
                let mapped = slice.get_mapped_range().map_err(ReadbackError::Range)?;
                crate::screenshot::strip_row_padding(&mapped, width, height)
            };
            crate::screenshot::to_rgba(self.format, pixels).map_err(ReadbackError::Decode)
        })();
        readback.unmap();
        result.map_err(OffscreenError::Readback)
    }

    /// Submit an offscreen capture and complete without blocking the browser
    /// main thread. `map_async` resolves through the WebGPU promise on the
    /// event loop (`Device::poll` is a no-op on wasm); `completion` receives
    /// the RGBA bytes — or the mapping error — with no `VfApp` borrow.
    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::too_many_arguments)]
    pub fn render_offscreen_async(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneGPU,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
        completion: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static,
    ) -> Result<(), String> {
        self.render_offscreen_async_with_objects(
            device, queue, scene, None, view_proj, camera_pos, width, height, clear, completion,
        )
    }

    /// As [`Self::render_offscreen_async`] with the object scene composited
    /// first.
    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::too_many_arguments)]
    pub fn render_offscreen_async_with_objects(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneGPU,
        objects: Option<&Arc<MeshScene>>,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
        completion: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static,
    ) -> Result<(), String> {
        let readback = self.submit_offscreen(
            device, queue, scene, objects, view_proj, camera_pos, width, height, clear,
        )?;
        let buffer = readback.clone();
        let format = self.format;
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                completion(readback_result(result, &buffer, format, width, height));
            });
        Ok(())
    }
}

/// Turn a wasm map callback into RGBA bytes, unmapping the buffer on every
/// path. Kept outside the callback so the failure paths stay readable.
#[cfg(target_arch = "wasm32")]
pub(crate) fn readback_result(
    result: Result<(), wgpu::BufferAsyncError>,
    buffer: &wgpu::Buffer,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    result.map_err(|error| format!("readback mapping failed: {error:?}"))?;
    let mapped = match buffer.get_mapped_range(..) {
        Ok(mapped) => mapped,
        Err(error) => {
            buffer.unmap();
            return Err(format!("readback range unavailable: {error}"));
        }
    };
    let pixels = crate::screenshot::strip_row_padding(&mapped, width, height);
    drop(mapped);
    buffer.unmap();
    crate::screenshot::to_rgba(format, pixels)
}

/// Per-frame draw: carries the scene to upload, the object scene to composite
/// first, and the camera matrices.
///
/// `object_style` is deferred to the persisted-style slice (#115): this slice
/// always renders the shipped Solid bodies.
pub struct DrawCallback {
    pub scene: SceneGPU,
    /// This frame's mapped source/induction bodies. `Arc` so a capture and the
    /// live callback share one fingerprint without a rebuild.
    pub objects: Arc<MeshScene>,
    /// The callback rect in egui points (used for the composite guard).
    pub screen: egui::Rect,
    /// The callback rect in physical pixels; the object target's extent.
    pub object_viewport: [u32; 2],
    /// Linear background the owned object target is cleared to.
    pub object_clear: [f64; 4],
    pub view_proj: Mat4,
    pub camera_pos: Vec3,
}

impl egui_wgpu::CallbackTrait for DrawCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let ds = callback_resources
            .get_mut::<DrawState>()
            .expect("DrawState must be installed before the first paint callback");
        ds.upload(device, queue, self.view_proj, self.camera_pos, &self.scene);

        // The object scene is installed before the pass so the paint stage can
        // name it; the pass itself records before any vector draw, in both the
        // live callback and `submit_offscreen`.
        ds.set_object_scene(Arc::clone(&self.objects));
        let object_view = ObjectView {
            view_proj: self.view_proj.to_cols_array_2d(),
            eye: self.camera_pos.to_array(),
            viewport: self.object_viewport,
            wireframe: false,
            clear: self.object_clear,
        };
        if let Err(error) =
            ds.object_pass
                .prepare(device, queue, egui_encoder, &self.objects, &object_view)
        {
            // A callback cannot return an error; the pass left `prepared` unset,
            // so `paint` skips the composite and the vector scene still paints.
            log::warn!("object pass skipped: {error}");
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let ds = callback_resources
            .get::<DrawState>()
            .expect("DrawState must be installed before the first paint callback");

        ds.record_with_objects(
            render_pass,
            &self.scene,
            self.objects.objects().len(),
            self.screen,
        );
    }
}

/// Painter's-algorithm helper: order a scene's instances far-to-near along the
/// view direction, since the egui_wgpu pass has no depth attachment.
///
/// Works on any instance mix: the ticket-62 combined stream carries arrows and
/// particle dots together, so the whole scene interleaves by depth. The sort is
/// stable, so equal-depth instances keep their pre-sort order. A non-finite
/// view depth (NaN or either infinity) is mapped to `NEG_INFINITY`: view-space
/// z grows toward the eye, so this puts it in the far group, first in the
/// painter order, instead of letting `INFINITY` paint on top. A bad origin can
/// therefore neither panic nor wedge itself in front of the scene.
pub fn sort_scene_back_to_front(instances: &mut [ArrowInstanceGPU], view: &Mat4) {
    instances.sort_by(|a, b| {
        let depth = |instance: &ArrowInstanceGPU| {
            let position = Vec3::new(instance.origin[0], instance.origin[1], instance.origin[2]);
            let z = (*view * position.extend(1.0)).z;
            if z.is_finite() { z } else { f32::NEG_INFINITY }
        };
        depth(a)
            .partial_cmp(&depth(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Painter's-algorithm helper for trail segments (ticket 48): order lines
/// far-to-near by the view depth of their midpoint, since alpha overlap is
/// order-visible. Stable, so equal-depth segments keep chronological order. A
/// non-finite midpoint depth joins the far group (`NEG_INFINITY`) instead of
/// painting in front or panicking.
pub fn sort_lines_back_to_front(instances: &mut [LineInstanceGPU], view: &Mat4) {
    instances.sort_by(|a, b| {
        let depth = |line: &LineInstanceGPU| {
            let midpoint = Vec3::new(
                (line.a[0] + line.b[0]) * 0.5,
                (line.a[1] + line.b[1]) * 0.5,
                (line.a[2] + line.b[2]) * 0.5,
            );
            let z = (*view * midpoint.extend(1.0)).z;
            if z.is_finite() { z } else { f32::NEG_INFINITY }
        };
        depth(a)
            .partial_cmp(&depth(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// A stable far-to-near copy of the trail list under the current view. The
/// app uses this each frame without writing the order back into the per-frame
/// trail history.
pub fn sorted_lines_back_to_front(lines: &[LineInstanceGPU], view: &Mat4) -> Vec<LineInstanceGPU> {
    let mut sorted = lines.to_vec();
    sort_lines_back_to_front(&mut sorted, view);
    sorted
}

/// The 2D arrow draw order: a stable far-to-near copy of the frame-invariant
/// arrow list under the current view. 2D keeps its own arrow2d → dots draws;
/// this helper reproduces the pre-ticket-62 sorted-arrow order without writing
/// the order back into the snapshot (the 3D combined stream is unaffected).
pub fn sorted_arrows_back_to_front(
    arrows: &[ArrowInstanceGPU],
    view: &Mat4,
) -> Vec<ArrowInstanceGPU> {
    let mut sorted = arrows.to_vec();
    sort_scene_back_to_front(&mut sorted, view);
    sorted
}

/// The long-standing arrow painter sort. Kept as a named alias so existing
/// callers/tests keep compiling; it is the same stable far-to-near comparator
/// as [`sort_scene_back_to_front`].
pub fn sort_back_to_front(instances: &mut [ArrowInstanceGPU], view: &Mat4) {
    sort_scene_back_to_front(instances, view);
}
