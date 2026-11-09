// Vector Field Explorer — instanced line shader.
//
// One camera-facing ribbon quad per segment, unlit flat color. Used for the
// reference grid, axes, and later streamline segments and particle trails.
//
// Bind group layout (same layout object is shared with the arrow pipeline):
//   (0) Globals uniform        — vertex
//   (1) instance storage array — vertex

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,    // xyz: direction *toward* the light (normalized)
    camera_right: vec4<f32>, // xyz: world-space camera right axis (dot billboards)
    camera_up: vec4<f32>,    // xyz: world-space camera up axis (dot billboards)
    arrow_style: vec4<u32>,  // x: style id, y: sides, z/w: unused
    arrow_params: vec4<f32>, // x: head length, y: head width, z: shaft radius, w: unused
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct LineInstance {
    a: vec4<f32>,       // xyz start, w = 1.0
    b: vec4<f32>,       // xyz end,   w = line width (world units)
    color: vec4<f32>,   // rgba 0..1
};

@group(0) @binding(1)
var<storage, read> line_instances: array<LineInstance>;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let inst_idx = vertex_index / 6u;
    let j = vertex_index % 6u;
    let inst = line_instances[inst_idx];

    let dir = inst.b.xyz - inst.a.xyz;
    let along = normalize(dir);
    let to_cam = normalize(globals.camera_pos.xyz - inst.a.xyz);
    let side = normalize(cross(along, to_cam));
    let half_w = inst.b.w * 0.5;

    // r: 0 (a,+) 1 (b,+) 2 (a,-) / 3 (b,+) 4 (b,-) 5 (a,-)
    let at_start = (j == 0u) || (j == 2u) || (j == 5u);
    let positive = (j == 0u) || (j == 1u) || (j == 3u);
    let p = select(inst.b.xyz, inst.a.xyz, at_start);
    let sgn = select(-1.0, 1.0, positive);

    var corner = p;
    if (length(dir) > 1e-8) {
        corner = p + side * half_w * sgn;
    }

    return VertexOut(
        globals.view_proj * vec4<f32>(corner, 1.0),
        inst.color,
    );
}

@fragment
fn fs_line(v: VertexOut) -> @location(0) vec4<f32> {
    return v.color;
}
