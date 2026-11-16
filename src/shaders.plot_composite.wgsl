// Vector Field Explorer — full-viewport plot composite shader.
//
// The plot depth pass renders into an owned color target. Egui's incoming paint
// pass has no depth attachment, so this tiny pipeline samples that target and
// covers the callback viewport with one full-rect triangle. It deliberately
// performs no manual gamma conversion: sampling an sRGB texture decodes once,
// and writing to an sRGB target encodes once. Linear targets stay linear.
//
// UV mapping: NDC (-1,-1) is the bottom-left of the viewport and texture v=1 is
// its bottom row, so v is inverted relative to NDC. That is not a vertical
// image flip; it places the target's top row at the viewport top.

@group(0) @binding(0)
var plot_color: texture_2d<f32>;

@group(0) @binding(1)
var plot_sampler: sampler;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_composite(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    // Oversized triangle covering the whole viewport.
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = positions[vertex_index];
    return VertexOut(
        vec4<f32>(p, 0.0, 1.0),
        vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5),
    );
}

@fragment
fn fs_composite(v: VertexOut) -> @location(0) vec4<f32> {
    // The plot target is cleared to the opaque viewport background; the
    // composite writes it back opaquely so no UI under the viewport leaks.
    let sampled = textureSample(plot_color, plot_sampler, v.uv);
    return vec4<f32>(sampled.rgb, 1.0);
}
