@group(0) @binding(0)
var ui: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let vertices = array<vec2f, 3>(vec2f(-1, -1), vec2f(3, -1), vec2f(-1, 3));
    return vec4f(vertices[index], 0, 1);
}

@fragment
fn fs(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let encoded = textureLoad(ui, vec2i(position.xy), 0);
    if TARGET_SRGB && encoded.a > 0.0 {
        // UI stores encoded-space premultiplied color. Hardware blending into
        // an sRGB target needs linear-space premultiplication instead.
        let straight = encoded.rgb / encoded.a;
        let linear = select(pow((straight + 0.055) / 1.055, vec3f(2.4)), straight / 12.92, straight <= vec3f(0.04045));
        return vec4f(linear * encoded.a, encoded.a);
    }
    return encoded;
}
