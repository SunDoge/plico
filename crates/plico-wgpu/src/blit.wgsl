@group(0) @binding(0)
var source: texture_2d<f32>;
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}
@vertex
fn vs(@builtin(vertex_index) index: u32) -> VertexOutput {
    let vertices = array<vec2f, 3>(vec2f(-1, -1), vec2f(3, -1), vec2f(-1, 3));
    var output: VertexOutput;
    output.position = vec4f(vertices[index], 0, 1);
    output.uv = vertices[index] * vec2f(0.5, -0.5) + vec2f(0.5);
    return output;
}
@fragment
fn fs(input: VertexOutput) -> @location(0) vec4f {
    let dimensions = vec2i(textureDimensions(source));
    let pixel = clamp(vec2i(input.uv * vec2f(dimensions)), vec2i(0), dimensions - vec2i(1));
    let encoded = textureLoad(source, pixel, 0);
    if TARGET_SRGB {
        // Preserve encoded-space composition on an sRGB swapchain. Decode the
        // stored premultiplied channels once; attachment encoding reverses it.
        let linear = select(
                pow((encoded.rgb + 0.055) / 1.055, vec3f(2.4)),
                encoded.rgb / 12.92,
                encoded.rgb <= vec3f(0.04045),
            );
        return vec4f(linear, encoded.a);
    }
    return encoded;
}
