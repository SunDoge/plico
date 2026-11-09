// Same area kernel/quantization as the single-surface render fallback.
@group(0) @binding(0)
var source: texture_2d_array<f32>;
@group(0) @binding(1)
var destination: texture_storage_2d_array<rgba8unorm, write>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= textureDimensions(destination)) {
        return;
    }
    let input_size = textureDimensions(source);
    let output_size = max(input_size / 2u, vec2u(1u));
    let xy = id.xy;
    let start = xy * input_size;
    let end = (xy + 1u) * input_size;
    let first = start / output_size;
    let last = (end + output_size - 1u) / output_size;
    var sum = vec4f(0.);
    for(var y = first.y; y < last.y; y++) {
        let wy = min((y + 1u) * output_size.y, end.y) - max(y * output_size.y, start.y);
        for(var x = first.x; x < last.x; x++) {
            let wx = min((x + 1u) * output_size.x, end.x) - max(x * output_size.x, start.x);
            sum += textureLoad(source, vec2i(i32(x), i32(y)), i32(id.z), 0) * f32(wx) * f32(wy);
        }
    }
    let average = sum / (f32(input_size.x) * f32(input_size.y));
    textureStore(destination, vec2i(id.xy), i32(id.z), floor(clamp(average, vec4f(0.), vec4f(1.)) * 255. + 0.5) / 255.);
}
