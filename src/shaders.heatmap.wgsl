// Vector Field Explorer — scalar heatmap quad shader (#92 slice B2).
//
// Six world-space vertices carry the plane-frame corners and their texture
// coordinates. The fragment samples the RGBA8 sRGB heatmap with a nearest
// sampler so transparent hole texels cannot bleed into painted cells.
//
// Bind group layout:
//   (0) Globals uniform          — vertex (view_projection at offset 0)
//   (1) heatmap texture          — fragment
//   (2) nearest sampler          — fragment

struct Globals {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(0) @binding(1)
var heatmap_texture: texture_2d<f32>;

@group(0) @binding(2)
var heatmap_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_heatmap(vertex: VertexIn) -> VertexOut {
    return VertexOut(
        globals.view_proj * vec4<f32>(vertex.position, 1.0),
        vertex.uv,
    );
}

@fragment
fn fs_heatmap(vertex: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(heatmap_texture, heatmap_sampler, vertex.uv);
}
