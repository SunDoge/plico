struct SimParams {
    drop: vec4f,
    old_sphere: vec4f,
    new_sphere: vec4f,
    control: vec4f,
}

@group(0) @binding(0)
var source: texture_2d<f32>;
@group(0) @binding(1)
var destination: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2)
var<uniform> params: SimParams;

fn load_height(p: vec2i, size: vec2i) -> f32 {
    return textureLoad(source, clamp(p, vec2i(0), size - 1), 0).x;
}

fn volume_in_sphere(uv: vec2f, sphere: vec4f) -> f32 {
    let surface = vec3f(uv.x * 2.0 - 1.0, 0.0, uv.y * 2.0 - 1.0);
    let to_center = surface - sphere.xyz;
    let t = length(to_center) / sphere.w;
    let dy = exp(-pow(t * 1.5, 6.0));
    let ymin = min(0.0, sphere.y - dy);
    let ymax = min(max(0.0, sphere.y + dy), ymin + 2.0 * dy);
    return (ymax - ymin) * 0.1;
}

@compute @workgroup_size(8, 8)
fn simulate(@builtin(global_invocation_id) id: vec3u) {
    let size = vec2i(textureDimensions(source));
    let p = vec2i(id.xy);
    if any(p >= size) {
        return;
    }

    var info = textureLoad(source, p, 0);
    let uv = (vec2f(p) + 0.5) / vec2f(size);
    let mode = i32(params.control.x + 0.5);
    if mode == 0 {
        let average =
            (load_height(p + vec2i(-1, 0), size) + load_height(p + vec2i(1, 0), size)
                + load_height(p + vec2i(0, -1), size) + load_height(p + vec2i(0, 1), size)) * 0.25;
        info.y += (average - info.x) * 2.0;
        info.y *= 0.995;
        info.x += info.y;
    } else if mode == 1 {
        var drop = max(0.0, 1.0 - distance(uv, params.drop.xy) / params.drop.z);
        drop = 0.5 - cos(drop * 3.14159265) * 0.5;
        info.x += drop * params.drop.w;
    } else if mode == 2 {
        let delta = 1.0 / vec2f(size);
        let dx = vec3f(delta.x, load_height(p + vec2i(1, 0), size) - info.x, 0.0);
        let dy = vec3f(0.0, load_height(p + vec2i(0, 1), size) - info.x, delta.y);
        let normal = normalize(cross(dy, dx));
        info.z = normal.x;
        info.w = normal.z;
    } else {
        info.x += volume_in_sphere(uv, params.old_sphere);
        info.x -= volume_in_sphere(uv, params.new_sphere);
    }
    textureStore(destination, p, info);
}
