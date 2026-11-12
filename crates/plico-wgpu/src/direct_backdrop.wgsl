struct PlicoDirectCompositeState {
    region: vec4f,
    clip: vec4f,
    mask: vec4f,
    flags: vec4f,
    blend: vec4f,
}
@group(2) @binding(0)
var<uniform> plico_direct_composite: PlicoDirectCompositeState;
@group(3) @binding(0)
var plico_direct_clip_mask: texture_2d<f32>;
@group(3) @binding(1)
var plico_direct_clip_sampler: sampler;

@fragment
fn fs_direct_composite(input: PlicoVertexOutput) -> @location(0) vec4f {
    let position = input.position.xy;
    let state = plico_direct_composite;
    let world = position + state.flags.xy;
    var coverage = 1.;
    if state.flags.w > 0.5 {
        let p = world - state.clip.xy;
        let c = clamp(min(state.clip.zw, p + 0.5) - max(vec2f(0.), p - 0.5), vec2f(0.), vec2f(1.));
        coverage = c.x * c.y;
    } else if any(world < state.clip.xy) || any(world >= state.clip.xy + state.clip.zw) {
        discard;
    }
    if state.flags.z > 0.5 {
        if any(world < state.mask.xy) || any(world >= state.mask.xy + state.mask.zw) {
            discard;
        }
        coverage *= textureSampleLevel(
                plico_direct_clip_mask,
                plico_direct_clip_sampler,
                (world - state.mask.xy) / state.mask.zw,
                0.,
            ).r;
    }
    if coverage <= 0. {
        discard;
    }
    let uv = (position - state.region.xy) / state.region.zw;
    let pixel = plico_effect(uv);
    var at = position - max(floor(state.region.xy), vec2f(0.));
    if state.blend.y > 0.5 {
        at = position + state.blend.zw;
    }
    let dimensions = vec2i(textureDimensions(plico_backdrop));
    let previous = textureLoad(plico_backdrop, clamp(vec2i(at), vec2i(0), dimensions - 1), 0);
    return mix(previous, pixel, coverage * state.blend.x);
}
