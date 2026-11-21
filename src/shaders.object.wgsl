// Vector Field Explorer — lit object body and fullscreen composite shader.
//
// A depth-writing body pipeline draws every mapped instance from the shared
// static primitive meshes (36-byte MeshVertex stride: position, outward normal,
// barycentric corner) with a 128-byte per-instance record (model matrix,
// inverse-transpose normal matrix, linear colour). Shading is ambient plus one
// directional diffuse term on a two-sided normal, with an optional world-space
// slice-plane discard and a barycentric wireframe path in the same entry point.
// A second depth-tested but non-depth-writing pipeline uses the attenuated
// fragment entry to draw the discarded slice half as a shaded body, so the 2D
// slice stays visible from the far side without hiding the kept half (#207).
//
// The second pair is the composite: egui's incoming paint pass has no depth
// attachment, so it samples the owned colour target and covers the callback
// viewport with one oversized triangle, using premultiplied alpha.
//
// Uniform ABI (128 bytes, `ObjectGlobals`):
//   view_proj   mat4x4<f32>  world -> clip (WebGPU [0,1] depth)
//   eye         vec4<f32>    camera position, w = 1
//   clip_plane  vec4<f32>    xyz normal, w offset
//   light_dir   vec4<f32>    xyz toward the light, w = 0
//   params      vec4<f32>    x wireframe, y clip enabled

struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    clip_plane: vec4<f32>,
    light_dir: vec4<f32>,
    params: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) barycentric: vec3<f32>,
};

struct InstanceIn {
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(7) normal_0: vec4<f32>,
    @location(8) normal_1: vec4<f32>,
    @location(9) normal_2: vec4<f32>,
    @location(10) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) barycentric: vec3<f32>,
    @location(3) color: vec4<f32>,
};

@vertex
fn vs_object(v: VertexIn, i: InstanceIn) -> VertexOut {
    let model = mat4x4<f32>(i.model_0, i.model_1, i.model_2, i.model_3);
    let normal_matrix = mat3x3<f32>(i.normal_0.xyz, i.normal_1.xyz, i.normal_2.xyz);
    let world = model * vec4<f32>(v.position, 1.0);
    return VertexOut(
        globals.view_proj * world,
        world.xyz,
        normalize(normal_matrix * v.normal),
        v.barycentric,
        i.color,
    );
}

@fragment
fn fs_object(v: VertexOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    // World-space slice discard: the visible half is `normal . p - offset >= 0`.
    if (globals.params.y > 0.5) {
        let distance = dot(v.world, globals.clip_plane.xyz) - globals.clip_plane.w;
        if (distance < 0.0) {
            discard;
        }
    }

    // Two-sided lighting: the normal always faces the camera.
    var normal = normalize(v.normal);
    if (!front_facing) {
        normal = -normal;
    }
    let light = normalize(globals.light_dir.xyz);
    let diffuse = max(dot(normal, light), 0.0);
    let shade = 0.25 + 0.75 * diffuse;

    if (globals.params.x > 0.5) {
        // Barycentric wireframe: a pixel within ~1.5 px of an edge survives.
        let width = fwidth(v.barycentric);
        let edges = v.barycentric / max(width, vec3<f32>(1e-5));
        let nearest = min(min(edges.x, edges.y), edges.z);
        if (nearest > 1.5) {
            discard;
        }
    }

    let rgb = clamp(v.color.rgb * shade, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(rgb, clamp(v.color.a, 0.0, 1.0));
}

// Issue #207: the 2D slice keeps the on-slice body authoritative but still
// shows the discarded side as a shaded body at reduced opacity, so a camera
// orbiting to the far side sees a surface rather than an empty cut. This entry
// reuses the same vertices and instance buffer; the pipeline disables depth
// writes so the kept half always stays visible. In Wireframe style the far side
// keeps the same edges-only reading as the rest of the object.
@fragment
fn fs_object_attenuated(v: VertexOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    if (globals.params.y <= 0.5) {
        discard;
    }
    let distance = dot(v.world, globals.clip_plane.xyz) - globals.clip_plane.w;
    if (distance >= 0.0) {
        discard;
    }

    if (globals.params.x > 0.5) {
        let width = fwidth(v.barycentric);
        let edges = v.barycentric / max(width, vec3<f32>(1e-5));
        let nearest = min(min(edges.x, edges.y), edges.z);
        if (nearest > 1.5) {
            discard;
        }
    }

    var normal = normalize(v.normal);
    if (!front_facing) {
        normal = -normal;
    }
    let light = normalize(globals.light_dir.xyz);
    let diffuse = max(dot(normal, light), 0.0);
    let rgb = clamp(v.color.rgb * (0.25 + 0.75 * diffuse), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(rgb, clamp(v.color.a, 0.0, 1.0) * 0.35);
}

@group(0) @binding(2)
var object_color: texture_2d<f32>;

@group(0) @binding(3)
var object_sampler: sampler;

struct CompositeOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_composite(@builtin(vertex_index) vertex_index: u32) -> CompositeOut {
    // Oversized triangle covering the whole viewport. NDC (-1,-1) is the
    // bottom-left, so v is inverted relative to NDC to put the target's top row
    // at the viewport top (no vertical image flip).
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = positions[vertex_index];
    return CompositeOut(
        vec4<f32>(p, 0.0, 1.0),
        vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5),
    );
}

@fragment
fn fs_composite(v: CompositeOut) -> @location(0) vec4<f32> {
    // Premultiplied output: the composite pipeline blends with
    // One / OneMinusSrcAlpha. Sampling an sRGB target decodes once and writing
    // to an sRGB target encodes once; linear targets stay linear.
    let sampled = textureSample(object_color, object_sampler, v.uv);
    return vec4<f32>(sampled.rgb * sampled.a, sampled.a);
}
