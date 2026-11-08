// Vector Field Explorer — instanced round dot (camera-facing billboard) shader.
//
// One dot per instance, geometry generated in the vertex shader from
// `vertex_index` (no vertex buffer): a camera-facing quad of two triangles,
// expanded along the Globals camera right/up axes by the instance's world-space
// radius (dir_len.w). The fragment keeps only the unit circle inscribed in that
// quad, shades a spherical normal from the quad's UV and fades the rim over one
// pixel, so a dot reads as a round disc from any orbit angle (ticket 29). This
// replaces the old 24-vertex octahedron, whose facets read as little diamonds.
//
// Bind group layout (same layout shape as the arrow and line pipelines):
//   (0) Globals uniform        — vertex+fragment
//   (1) instance storage array — vertex

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,    // xyz: direction *toward* the light (normalized)
    camera_right: vec4<f32>, // xyz: world-space camera right axis (w = 0)
    camera_up: vec4<f32>,    // xyz: world-space camera up axis (w = 0)
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct DotInstance {
    origin: vec4<f32>,   // xyz center position, w = 1.0
    dir_len: vec4<f32>,  // xyz: unused, w = dot radius (world units)
    color: vec4<f32>,    // rgba 0..1
};

@group(0) @binding(1)
var<storage, read> dot_instances: array<DotInstance>;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>, // dot-local disc coordinate, |uv| <= 1 inside
};

// Two triangles, six vertices, no index buffer. Mirrors DOT_QUAD_CORNERS in
// src/particles.rs (the CPU side of the same geometry).
const DOT_VERTS: u32 = 6u;

// Corner `j` of the billboard quad in [-1,1] dot-local space:
//   (-1,-1) (1,-1) (1,1) / (-1,-1) (1,1) (-1,1)
// so the unit circle inscribed in the square is the dot's silhouette.
fn quad_corner(j: u32) -> vec2<f32> {
    let min_x = (j == 0u) || (j == 3u) || (j == 5u);
    let min_y = (j == 0u) || (j == 1u) || (j == 3u);
    return vec2<f32>(select(1.0, -1.0, min_x), select(1.0, -1.0, min_y));
}

@vertex
fn vs_dot(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let inst_idx = vertex_index / DOT_VERTS;
    let j = vertex_index % DOT_VERTS;
    let inst = dot_instances[inst_idx];

    // Camera-facing expansion: the quad lives in the plane spanned by the
    // camera's right/up axes, so it always faces the eye and keeps the
    // instance's world-space radius (screen size still shrinks with distance).
    let corner = quad_corner(j);
    let right = globals.camera_right.xyz;
    let up = globals.camera_up.xyz;
    let radius = inst.dir_len.w;
    let world = inst.origin.xyz + (right * corner.x + up * corner.y) * radius;

    return VertexOut(
        globals.view_proj * vec4<f32>(world, 1.0),
        inst.color,
        corner,
    );
}

@fragment
fn fs_dot(v: VertexOut) -> @location(0) vec4<f32> {
    let r = length(v.uv);
    // One-pixel rim: the quad is the disc's bounding square, so the fade runs
    // inward and reaches zero coverage at r == 1. The derivative is taken
    // before the discard (derivatives in divergent control flow are
    // implementation-defined) and floored so a degenerate derivative cannot
    // make smoothstep's edges cross.
    let aa = max(fwidth(r), 1e-5);
    // No square halo: everything outside the disc is dropped.
    if (r > 1.0) {
        discard;
    }
    let coverage = 1.0 - smoothstep(1.0 - aa, 1.0, r);

    // Spherical normal in the camera basis. It is unit length by construction
    // (uv.x^2 + uv.y^2 + nz^2 == 1), so no normalize is needed — and none at
    // the rim, where a normalize would divide by zero.
    let nz = sqrt(max(1.0 - r * r, 0.0));
    let normal = globals.camera_right.xyz * v.uv.x
        + globals.camera_up.xyz * v.uv.y
        + cross(globals.camera_right.xyz, globals.camera_up.xyz) * nz;
    let nl = clamp(dot(normal, globals.light_dir.xyz) * 0.5 + 0.5, 0.0, 1.0);

    // Same half-lambert falloff as the arrows; the alpha ramp is the rim
    // antialiasing (the dot pipeline blends, so edge pixels fade into whatever
    // is behind them instead of leaving a stair-stepped silhouette).
    let lit = v.color.rgb * (0.35 + 0.65 * nl);
    return vec4<f32>(lit, v.color.a * coverage);
}
