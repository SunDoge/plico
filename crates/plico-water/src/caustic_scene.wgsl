struct CausticUniforms {
    light: vec4f,
    sphere: vec4f,
}

@group(0) @binding(0) var<uniform> scene: CausticUniforms;
@group(0) @binding(1) var water: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;

const IOR_AIR: f32 = 1.0;
const IOR_WATER: f32 = 1.333;

struct VertexOut {
    @builtin(position) clip_position: vec4f,
    @location(0) old_position: vec3f,
    @location(1) new_position: vec3f,
    @location(2) ray: vec3f,
}

fn intersect_cube(origin: vec3f, ray: vec3f, low: vec3f, high: vec3f) -> vec2f {
    let t_min = (low - origin) / ray;
    let t_max = (high - origin) / ray;
    let t1 = min(t_min, t_max);
    let t2 = max(t_min, t_max);
    return vec2f(max(max(t1.x, t1.y), t1.z), min(min(t2.x, t2.y), t2.z));
}

fn project_to_floor(origin_value: vec3f, ray: vec3f, refracted_light: vec3f) -> vec3f {
    var origin = origin_value;
    let range = intersect_cube(origin, ray, vec3f(-1.0), vec3f(1.0, 2.0, 1.0));
    origin += ray * range.y;
    let distance = (-origin.y - 1.0) / refracted_light.y;
    return origin + refracted_light * distance;
}

@vertex
fn vertex(@location(0) plane_position: vec2f) -> VertexOut {
    let info = textureSampleLevel(water, linear_sampler, plane_position * 0.5 + 0.5, 0.0);
    let flattened_normal = info.zw * 0.5;
    let normal = vec3f(flattened_normal.x, sqrt(max(0.0, 1.0 - dot(flattened_normal, flattened_normal))), flattened_normal.y);
    let refracted_light = refract(-scene.light.xyz, vec3f(0.0, 1.0, 0.0), IOR_AIR / IOR_WATER);
    let ray = refract(-scene.light.xyz, normal, IOR_AIR / IOR_WATER);
    let origin = vec3f(plane_position.x, 0.0, plane_position.y);
    let old_position = project_to_floor(origin, refracted_light, refracted_light);
    let new_position = project_to_floor(origin + vec3f(0.0, info.x, 0.0), ray, refracted_light);
    var out: VertexOut;
    out.old_position = old_position;
    out.new_position = new_position;
    out.ray = ray;
    out.clip_position = vec4f(0.75 * (new_position.xz + refracted_light.xz / refracted_light.y), 0.0, 1.0);
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4f {
    let old_area = length(dpdx(in.old_position)) * length(dpdy(in.old_position));
    let new_area = max(length(dpdx(in.new_position)) * length(dpdy(in.new_position)), 1.0e-6);
    var focus = old_area / new_area * 0.2;
    let refracted_light = refract(-scene.light.xyz, vec3f(0.0, 1.0, 0.0), IOR_AIR / IOR_WATER);
    let direction = (scene.sphere.xyz - in.new_position) / scene.sphere.w;
    let area = cross(direction, refracted_light);
    var shadow = dot(area, area);
    let distance = dot(direction, -refracted_light);
    shadow = 1.0 + (shadow - 1.0) / (0.05 + distance * 0.025);
    shadow = clamp(1.0 / (1.0 + exp(-shadow)), 0.0, 1.0);
    shadow = mix(1.0, shadow, clamp(distance * 2.0, 0.0, 1.0));
    let range = intersect_cube(in.new_position, -refracted_light, vec3f(-1.0), vec3f(1.0, 2.0, 1.0));
    focus *= 1.0 / (1.0 + exp(-200.0 / (1.0 + 10.0 * (range.y - range.x)) * (in.new_position.y - refracted_light.y * range.y - 2.0 / 12.0)));
    return vec4f(focus, shadow, 0.0, 0.0);
}
