@group(0) @binding(0)
var atlas: texture_2d<f32>;
@group(0) @binding(1)
var atlas_sampler: sampler;
struct Out {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}
@vertex
fn vs(
    @builtin(vertex_index) index: u32,
    @location(0) rect: vec4f,
    @location(1) uv: vec4f,
    @location(2) color: vec4f,
) -> Out {
    let corners = array<vec2f, 6>(vec2f(0, 0), vec2f(1, 0), vec2f(0, 1), vec2f(0, 1), vec2f(1, 0), vec2f(1, 1));
    let p = corners[index];
    var out: Out;
    // CPU supplies normalized device coordinates, including downward-positive Y.
    out.position = vec4f(rect.xy + p * rect.zw, 0, 1);
    out.uv = uv.xy + p * uv.zw;
    out.color = color;
    return out;
}
@fragment
fn fs(input: Out) -> @location(0) vec4f {
    let sample = textureSample(atlas, atlas_sampler, input.uv);
    return sample * input.color;
}

// Same coverage, different blend space. No coverage sharpening/emboldening.
@fragment
fn fs_linear(input: Out) -> @location(0) vec4f {
    let sample = textureSample(atlas, atlas_sampler, input.uv);
    let encoded = sample.rgb / max(sample.a, 0.000001) * input.color.rgb;
    let rgb = select(pow((encoded + 0.055) / 1.055, vec3f(2.4)), encoded / 12.92, encoded <= vec3f(0.04045));
    let a = sample.a * input.color.a;
    return vec4f(rgb * a, a);
}
