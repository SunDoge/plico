struct State { region: vec4f, clip: vec4f, mask: vec4f, flags: vec4f }
@group(0) @binding(0) var output: texture_2d<f32>;
@group(0) @binding(1) var backdrop: texture_2d<f32>;
@group(0) @binding(2) var<uniform> state: State;
@group(1) @binding(0) var clip_mask: texture_2d<f32>;
@group(1) @binding(1) var clip_sampler: sampler;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let p = array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
    return vec4f(p[i],0.,1.);
}
@fragment fn fs(@builtin(position) position:vec4f)->@location(0) vec4f {
    let world = position.xy + state.flags.xy;
    var coverage = 1.;
    if state.flags.w > 0.5 {
        let p = world - state.clip.xy;
        let c = clamp(min(state.clip.zw, p + 0.5) - max(vec2f(0.),p - 0.5),vec2f(0.),vec2f(1.));
        coverage = c.x*c.y;
    } else if any(world < state.clip.xy) || any(world >= state.clip.xy + state.clip.zw) { discard; }
    if state.flags.z > 0.5 {
        if any(world < state.mask.xy) || any(world >= state.mask.xy + state.mask.zw) { discard; }
        coverage *= textureSampleLevel(clip_mask,clip_sampler,(world-state.mask.xy)/state.mask.zw,0.).r;
    }
    if coverage <= 0. { discard; }
    let uv = (position.xy-state.region.xy)/state.region.zw;
    let dimensions = vec2i(textureDimensions(output));
    let pixel = textureLoad(output, clamp(vec2i(uv * vec2f(dimensions)),vec2i(0),dimensions-1),0);
    let previous = textureLoad(backdrop, vec2i(position.xy-max(floor(state.region.xy),vec2f(0.))),0);
    return mix(previous,pixel,coverage);
}
