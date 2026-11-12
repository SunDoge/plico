// Render-pass Gaussian blur for backends without compute (WebGL2). Same
// separable convolution and edge modes as the compute pipeline in blur.wgsl;
// a fullscreen triangle loads the source row/column and writes the result.
struct Params {
    sigma: f32,
    axis: u32,
    edge: u32,
    padding: u32,
}
@group(0) @binding(0)
var source: texture_2d<f32>;
@group(0) @binding(1)
var<uniform> params: Params;
fn coordinate(x: i32, n: i32) -> i32 {
    if params.edge == 1u {
        return clamp(x, 0, n - 1);
    }
    if params.edge == 2u {
        return ((x % n) + n) % n;
    }
    let v = ((x % (2 * n)) + 2 * n) % (2 * n);
    return min(v, 2 * n - 1 - v);
}
@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let p = array<vec2f, 3>(vec2f(-1., -1.), vec2f(3., -1.), vec2f(-1., 3.));
    return vec4f(p[index], 0., 1.);
}
@fragment
fn fs(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let size = vec2i(textureDimensions(source));
    let pixel = vec2i(position.xy);
    if any(pixel >= size) {
        return vec4f(0.);
    }
    let radius = i32(select(u32(ceil(4. * params.sigma)), params.padding, params.padding > 0u));
    // Uniform for the pass; hoist out of the tap loop.
    let falloff = -0.5 / (params.sigma * params.sigma);
    let delta = select(vec2i(0, 1), vec2i(1, 0), params.axis == 0u);
    var sum = vec4f(0.);
    var weight = 0.;
    for(var i = -radius; i <= radius; i++) {
        let w = exp(falloff * f32(i * i));
        var p = pixel + delta * i;
        weight += w;
        if params.edge == 0u && (any(p < vec2i(0)) || any(p >= size)) {
            continue;
        }
        if params.edge != 0u {
            p = vec2i(coordinate(p.x, size.x), coordinate(p.y, size.y));
        }
        sum += textureLoad(source, p, 0) * w;
    }
    return sum / weight;
}
