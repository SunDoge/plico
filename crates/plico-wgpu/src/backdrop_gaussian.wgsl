struct Params {
    down: u32,
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
}
@group(0) @binding(0)
var source: texture_2d<f32>;
@group(0) @binding(1)
var destination: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2)
var<uniform> params: Params;

// MyGO BackdropOf averages Down×Down source squares before the Gaussian.
// The last row or column averages only the pixels that remain.
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let output_size = textureDimensions(destination);
    if any(id.xy >= output_size) {
        return;
    }
    let input_size = textureDimensions(source);
    let first = id.xy * params.down;
    let last = min(first + vec2u(params.down), input_size);
    var sum = vec4f(0.);
    var count = 0u;
    for(var y = first.y; y < last.y; y++) {
        for(var x = first.x; x < last.x; x++) {
            sum += textureLoad(source, vec2i(i32(x), i32(y)), 0);
            count += 1u;
        }
    }
    let average = sum / f32(max(count, 1u));
    textureStore(destination, vec2i(id.xy), floor(clamp(average, vec4f(0.), vec4f(1.)) * 255. + 0.5) / 255.);
}
