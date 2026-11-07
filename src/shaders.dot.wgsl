// Vector Field Explorer — instanced dot (octahedron) shader.
//
// One small faceted "dot" per instance, geometry generated in the
// vertex shader from `vertex_index` (no vertex buffer). The octahedron is
// centered on the instance origin with radius instance.dir_len.w, so a dot
// is camera-independent in screen size (world-space radius). Half-lambert
// lit by Globals.light_dir, same as the arrow shader.
//
// Bind group layout (same layout shape as the arrow and line pipelines):
//   (0) Globals uniform        — vertex+fragment
//   (1) instance storage array — vertex

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>, // xyz: direction *toward* the light (normalized)
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct DotInstance {
    origin: vec4<f32>,   // xyz center position, w = 1.0
    dir_len: vec4<f32>,  // xyz: unused, w = dot radius
    color: vec4<f32>,    // rgba 0..1
};

@group(0) @binding(1)
var<storage, read> dot_instances: array<DotInstance>;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) light: f32, // 0..1 half-lambert factor
};

// Geometry layout per instance (triangle list, no vertex buffer):
//   verts 0..24 : octahedron, 8 faces x 3 verts. Face f connects the three
//   poles (sx,0,0), (0,sy,0), (0,0,sz) where (sx,sy,sz) in {-1,1}^3 are
//   chosen from f's three bits; corner c picks which pole of the face it is.
//   8 faces x 3 corners = 24 verts, winding irrelevant (cull_mode: None).
const DOT_VERTS: u32 = 24u;

// Octahedron corner `corner` of face `face`, in [-1,1] dot-local space, and
// the face's outward normal (shared by all three corners of the face).
fn dot_vertex(face: u32, corner: u32) -> vec3<f32> {
    let sx = select(-1.0, 1.0, (face & 1u) != 0u);
    let sy = select(-1.0, 1.0, (face & 2u) != 0u);
    let sz = select(-1.0, 1.0, (face & 4u) != 0u);
    if (corner == 0u) {
        return vec3<f32>(sx, 0.0, 0.0);
    } else if (corner == 1u) {
        return vec3<f32>(0.0, sy, 0.0);
    }
    return vec3<f32>(0.0, 0.0, sz);
}

@vertex
fn vs_dot(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let inst_idx = vertex_index / DOT_VERTS;
    let j = vertex_index % DOT_VERTS;
    let inst = dot_instances[inst_idx];

    let face = j / 3u;
    let corner = j % 3u;
    let sx = select(-1.0, 1.0, (face & 1u) != 0u);
    let sy = select(-1.0, 1.0, (face & 2u) != 0u);
    let sz = select(-1.0, 1.0, (face & 4u) != 0u);

    let local = dot_vertex(face, corner);
    let normal = normalize(vec3<f32>(sx, sy, sz));
    let radius = inst.dir_len.w;
    let world = inst.origin.xyz + local * radius;

    let nl = clamp(dot(normal, globals.light_dir.xyz) * 0.5 + 0.5, 0.0, 1.0);

    return VertexOut(
        globals.view_proj * vec4<f32>(world, 1.0),
        inst.color,
        nl,
    );
}

@fragment
fn fs_dot(v: VertexOut) -> @location(0) vec4<f32> {
    let lit = v.color.rgb * (0.35 + 0.65 * v.light);
    return vec4<f32>(lit, v.color.a);
}
