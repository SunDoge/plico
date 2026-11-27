override DOWN: u32;
@group(0) @binding(0)
var source: texture_2d<f32>;
@group(0) @binding(1)
var destination: texture_storage_2d<rgba8unorm, write>;
// MyGO BackdropOf averages Down×Down source squares before the Gaussian.
// The fixed path only dispatches full source squares.
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let output_size = textureDimensions(destination);
    if any(id.xy >= output_size) {
        return;
    }
    let first = id.xy * DOWN;
    var sum = vec4f(0.);
    for(var y = 0u; y < DOWN; y++) {
        for(var x = 0u; x < DOWN; x++) {
            sum += textureLoad(source, vec2i(first + vec2u(x, y)), 0);
        }
    }
    let average = sum / f32(DOWN * DOWN);
    textureStore(destination, vec2i(id.xy), floor(clamp(average, vec4f(0.), vec4f(1.)) * 255. + 0.5) / 255.);
}
