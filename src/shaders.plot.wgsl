// Vector Field Explorer — lit scalar surface shader.
//
// Indexed triangle mesh (PlotVertex, 48-byte stride). The bind group layout and
// Globals uniform ABI are shared with the unchanged line shader; only the
// vertex inputs and fragment lighting differ. Shading is ambient plus one
// directional diffuse term, exactly as specified: no specular, no material
// controls, and an opaque alpha so holes stay holes.
//
// Bind group layout (same layout object is shared with the plot line pipelines):
//   (0) Globals uniform — vertex + fragment (light_dir)
//   (1) unused storage array — kept so one bind-group layout serves the pass

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,    // xyz: direction *toward* the light (normalized)
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
    arrow_style: vec4<u32>,
    arrow_params: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexIn {
    @location(0) position: vec4<f32>, // xyz, w = 1
    @location(1) normal: vec4<f32>,   // xyz world-space normal, w = 0
    @location(2) color: vec4<f32>,    // linear rgba base colour
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_mesh(v: VertexIn) -> VertexOut {
    return VertexOut(
        globals.view_proj * vec4<f32>(v.position.xyz, 1.0),
        v.normal.xyz,
        v.color,
    );
}

@fragment
fn fs_mesh(v: VertexOut) -> @location(0) vec4<f32> {
    let normal = normalize(v.normal);
    let light = normalize(globals.light_dir.xyz);
    let diffuse = max(dot(normal, light), 0.0);
    let shade = 0.25 + 0.75 * diffuse;
    return vec4<f32>(v.color.rgb * shade, 1.0);
}
