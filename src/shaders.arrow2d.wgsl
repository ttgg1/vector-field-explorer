// Vector Field Explorer — flat 2D arrow shader (ticket 45).
//
// One flat arrow per instance, geometry generated in the vertex shader from
// `vertex_index` (no vertex buffer). The arrow lives in world XY: the instance
// direction is projected onto the view plane, the shaft is a quad and the head
// a triangle. The 2D camera looks down -Z, so no perspective foreshortening or
// lighting applies; the fragment returns the colormap colour unchanged.
//
// Bind group layout (same layout object is shared with the 3D arrow, line and
// dot pipelines):
//   (0) Globals uniform        — vertex
//   (1) instance storage array — vertex

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,    // xyz: direction *toward* the light (normalized)
    camera_right: vec4<f32>, // xyz: world-space camera right axis (w = 0)
    camera_up: vec4<f32>,    // xyz: world-space camera up axis (w = 0)
    arrow_style: vec4<u32>,  // x: style id, y: sides, z/w: unused
    arrow_params: vec4<f32>, // x: head length, y: head width, z: shaft radius, w: unused
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct ArrowInstance {
    origin: vec4<f32>,   // xyz base position, w = 1.0
    dir_len: vec4<f32>,  // xyz unit direction, w = length
    color: vec4<f32>,    // rgba 0..1
};

@group(0) @binding(1)
var<storage, read> arrow_instances: array<ArrowInstance>;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// Geometry layout per instance (triangle list, no vertex buffer):
//   verts 0..6 : shaft quad, two triangles
//   verts 6..9 : head triangle
const ARROW2D_VERTS: u32 = 9u;

@vertex
fn vs_arrow2d(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let inst_idx = vertex_index / ARROW2D_VERTS;
    let j = vertex_index % ARROW2D_VERTS;
    let inst = arrow_instances[inst_idx];

    let params = globals.arrow_params;
    let len = inst.dir_len.w;
    let shaft_len = (1.0 - params.x) * len;
    let half_shaft = params.z * len;
    let half_head = params.y * len;

    // Project the sampled direction onto the view plane. A zero XY direction
    // falls back to +Y so a pure-depth arrow still has a visible silhouette.
    var axis = vec2<f32>(inst.dir_len.x, inst.dir_len.y);
    if (length(axis) < 1e-5) {
        axis = vec2<f32>(0.0, 1.0);
    } else {
        axis = normalize(axis);
    }
    let side = vec2<f32>(-axis.y, axis.x);

    let base = inst.origin.xy;
    let junction = base + axis * shaft_len;
    let tip = base + axis * len;

    var p: vec2<f32>;
    if (j < 6u) {
        // r: 0 (base,+) 1 (junction,+) 2 (base,-) / 3 (junction,+) 4 (junction,-) 5 (base,-)
        let at_start = (j == 0u) || (j == 2u) || (j == 5u);
        let positive = (j == 0u) || (j == 1u) || (j == 3u);
        let along = select(junction, base, at_start);
        let sgn = select(-1.0, 1.0, positive);
        p = along + side * half_shaft * sgn;
    } else if (j == 6u) {
        p = junction + side * half_head;
    } else if (j == 7u) {
        p = junction - side * half_head;
    } else {
        p = tip;
    }

    let world = vec3<f32>(p.x, p.y, inst.origin.z);
    return VertexOut(globals.view_proj * vec4<f32>(world, 1.0), inst.color);
}

@fragment
fn fs_arrow2d(v: VertexOut) -> @location(0) vec4<f32> {
    // Flat, unlit: 2D shading would only make the sampled colour harder to
    // compare with the colormap.
    return v.color;
}
