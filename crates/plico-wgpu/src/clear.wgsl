struct Frame { size_time_scale: vec4f, background: vec4f }
@group(0) @binding(0) var<uniform> frame: Frame;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let p = array<vec2f, 3>(vec2f(-1., -1.), vec2f(3., -1.), vec2f(-1., 3.));
    return vec4f(p[index], 0., 1.);
}
@fragment fn fs() -> @location(0) vec4f { return frame.background; }
