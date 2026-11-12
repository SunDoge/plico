struct SceneUniforms {
    view_projection: mat4x4f,
    eye: vec4f,
    light: vec4f,
    sphere: vec4f,
}

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;
@group(0) @binding(1)
var water: texture_2d<f32>;
@group(0) @binding(2)
var caustics: texture_2d<f32>;
@group(0) @binding(3)
var tiles: texture_2d<f32>;
@group(0) @binding(4)
var sky: texture_cube<f32>;
@group(0) @binding(5)
var linear_sampler: sampler;

const IOR_AIR: f32 = 1.0;
const IOR_WATER: f32 = 1.333;
const ABOVE_WATER_COLOR: vec3f = vec3f(0.25, 1.0, 1.25);
const UNDERWATER_COLOR: vec3f = vec3f(0.4, 0.9, 1.0);
const POOL_HEIGHT: f32 = 1.0;

struct VertexOut {
    @builtin(position) clip_position: vec4f,
    @location(0) world_position: vec3f,
}

fn intersect_cube(origin: vec3f, ray: vec3f, low: vec3f, high: vec3f) -> vec2f {
    let t_min = (low - origin) / ray;
    let t_max = (high - origin) / ray;
    let t1 = min(t_min, t_max);
    let t2 = max(t_min, t_max);
    return vec2f(max(max(t1.x, t1.y), t1.z), min(min(t2.x, t2.y), t2.z));
}

fn intersect_sphere(origin: vec3f, ray: vec3f) -> f32 {
    let to_sphere = origin - scene.sphere.xyz;
    let a = dot(ray, ray);
    let b = 2.0 * dot(to_sphere, ray);
    let c = dot(to_sphere, to_sphere) - scene.sphere.w * scene.sphere.w;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant > 0.0 {
        let distance = (-b - sqrt(discriminant)) / (2.0 * a);
        if distance > 0.0 {
            return distance;
        }
    }
    return 1.0e6;
}

fn tile_pattern(point: vec3f, normal: vec3f) -> vec3f {
    var coordinates: vec2f;
    if abs(normal.x) > 0.5 {
        coordinates = point.yz * 0.5 + vec2f(1.0, 0.5);
    } else if abs(normal.z) > 0.5 {
        coordinates = point.yx * 0.5 + vec2f(1.0, 0.5);
    } else {
        coordinates = point.xz * 0.5 + 0.5;
    }
    return textureSample(tiles, linear_sampler, coordinates).rgb;
}

fn caustic_sample(point: vec3f, refracted_light: vec3f) -> vec2f {
    let uv = 0.75 * (point.xz - point.y * refracted_light.xz / refracted_light.y) * 0.5 + 0.5;
    return textureSample(caustics, linear_sampler, uv).rg;
}

fn sphere_color(point: vec3f) -> vec3f {
    let radius = scene.sphere.w;
    var color = vec3f(0.5);
    color *= 1.0 - 0.9 / pow((1.0 + radius - abs(point.x)) / radius, 3.0);
    color *= 1.0 - 0.9 / pow((1.0 + radius - abs(point.z)) / radius, 3.0);
    color *= 1.0 - 0.9 / pow((point.y + 1.0 + radius) / radius, 3.0);
    let normal = (point - scene.sphere.xyz) / radius;
    let refracted_light = refract(-scene.light.xyz, vec3f(0.0, 1.0, 0.0), IOR_AIR / IOR_WATER);
    var diffuse = max(0.0, dot(-refracted_light, normal)) * 0.5;
    let water_info = textureSample(water, linear_sampler, point.xz * 0.5 + 0.5);
    if point.y < water_info.x {
        diffuse *= caustic_sample(point, refracted_light).x * 4.0;
    }
    return color + diffuse;
}

fn wall_color(point: vec3f) -> vec3f {
    var normal: vec3f;
    if abs(point.x) > 0.999 {
        normal = vec3f(-point.x, 0.0, 0.0);
    } else if abs(point.z) > 0.999 {
        normal = vec3f(0.0, 0.0, -point.z);
    } else {
        normal = vec3f(0.0, 1.0, 0.0);
    }
    var scale = 0.5 / length(point);
    scale *= 1.0 - 0.9 / pow(length(point - scene.sphere.xyz) / scene.sphere.w, 4.0);
    let refracted_light = -refract(-scene.light.xyz, vec3f(0.0, 1.0, 0.0), IOR_AIR / IOR_WATER);
    var diffuse = max(0.0, dot(refracted_light, normal));
    let water_info = textureSample(water, linear_sampler, point.xz * 0.5 + 0.5);
    if point.y < water_info.x {
        let caustic = caustic_sample(point, refracted_light);
        scale += diffuse * caustic.x * 2.0 * caustic.y;
    } else {
        let range = intersect_cube(point, refracted_light, vec3f(-1.0, -POOL_HEIGHT, -1.0), vec3f(1.0, 2.0, 1.0));
        diffuse *= 1.0 / (1.0 + exp(
                    -200.0 / (1.0 + 10.0 * (range.y - range.x)) * (point.y + refracted_light.y * range.y - 2.0 / 12.0),
                ));
        scale += diffuse * 0.5;
    }
    return tile_pattern(point, normal) * scale;
}

fn sky_color(ray: vec3f) -> vec3f {
    var color = textureSample(sky, linear_sampler, ray).rgb;
    color += vec3f(10.0, 8.0, 6.0) * pow(max(0.0, dot(scene.light.xyz, ray)), 5000.0);
    return color;
}

fn surface_ray_color(origin: vec3f, ray: vec3f, water_color: vec3f) -> vec3f {
    let sphere_distance = intersect_sphere(origin, ray);
    var color: vec3f;
    if sphere_distance < 1.0e6 {
        color = sphere_color(origin + ray * sphere_distance);
    } else if ray.y < 0.0 {
        let range = intersect_cube(origin, ray, vec3f(-1.0, -POOL_HEIGHT, -1.0), vec3f(1.0, 2.0, 1.0));
        color = wall_color(origin + ray * range.y);
    } else {
        let range = intersect_cube(origin, ray, vec3f(-1.0, -POOL_HEIGHT, -1.0), vec3f(1.0, 2.0, 1.0));
        let hit = origin + ray * range.y;
        if hit.y < 2.0 / 12.0 {
            color = wall_color(hit);
        } else {
            color = sky_color(ray);
        }
    }
    if ray.y < 0.0 {
        color *= water_color;
    }
    return color;
}

@vertex
fn pool_vertex(@location(0) source_position: vec3f) -> VertexOut {
    var out: VertexOut;
    var position = source_position;
    position.y = ((1.0 - position.y) * (7.0 / 12.0) - 1.0) * POOL_HEIGHT;
    out.world_position = position;
    out.clip_position = scene.view_projection * vec4f(position, 1.0);
    return out;
}

@fragment
fn pool_fragment(in: VertexOut) -> @location(0) vec4f {
    var color = wall_color(in.world_position);
    let info = textureSample(water, linear_sampler, in.world_position.xz * 0.5 + 0.5);
    if in.world_position.y < info.x {
        color *= UNDERWATER_COLOR * 1.2;
    }
    return vec4f(color, 1.0);
}

@vertex
fn sphere_vertex(@location(0) unit_position: vec3f) -> VertexOut {
    var out: VertexOut;
    let position = scene.sphere.xyz + unit_position * scene.sphere.w;
    out.world_position = position;
    out.clip_position = scene.view_projection * vec4f(position, 1.0);
    return out;
}

@fragment
fn sphere_fragment(in: VertexOut) -> @location(0) vec4f {
    var color = sphere_color(in.world_position);
    let info = textureSample(water, linear_sampler, in.world_position.xz * 0.5 + 0.5);
    if in.world_position.y < info.x {
        color *= UNDERWATER_COLOR * 1.2;
    }
    return vec4f(color, 1.0);
}

@vertex
fn water_vertex(@location(0) plane_position: vec2f) -> VertexOut {
    var out: VertexOut;
    let info = textureSampleLevel(water, linear_sampler, plane_position * 0.5 + 0.5, 0.0);
    let position = vec3f(plane_position.x, info.x, plane_position.y);
    out.world_position = position;
    out.clip_position = scene.view_projection * vec4f(position, 1.0);
    return out;
}

fn shade_water(position: vec3f, below: bool) -> vec4f {
    var coordinate = position.xz * 0.5 + 0.5;
    var info = textureSample(water, linear_sampler, coordinate);
    for(var index = 0; index < 5; index += 1) {
        coordinate += info.zw * 0.005;
        info = textureSample(water, linear_sampler, coordinate);
    }
    var normal = vec3f(info.z, sqrt(max(0.0, 1.0 - dot(info.zw, info.zw))), info.w);
    let incoming = normalize(position - scene.eye.xyz);
    var color: vec3f;
    if below {
        normal = -normal;
        let reflected_ray = reflect(incoming, normal);
        let refracted_ray = refract(incoming, normal, IOR_WATER / IOR_AIR);
        let fresnel = mix(0.5, 1.0, pow(1.0 - dot(normal, -incoming), 3.0));
        let reflected = surface_ray_color(position, reflected_ray, UNDERWATER_COLOR);
        let refracted = surface_ray_color(position, refracted_ray, vec3f(1.0)) * vec3f(0.8, 1.0, 1.1);
        color = mix(reflected, refracted, (1.0 - fresnel) * length(refracted_ray));
    } else {
        let reflected_ray = reflect(incoming, normal);
        let refracted_ray = refract(incoming, normal, IOR_AIR / IOR_WATER);
        let fresnel = mix(0.25, 1.0, pow(1.0 - dot(normal, -incoming), 3.0));
        let reflected = surface_ray_color(position, reflected_ray, ABOVE_WATER_COLOR);
        let refracted = surface_ray_color(position, refracted_ray, ABOVE_WATER_COLOR);
        color = mix(refracted, reflected, fresnel);
    }
    return vec4f(color, 1.0);
}

@fragment
fn water_above(in: VertexOut) -> @location(0) vec4f {
    return shade_water(in.world_position, false);
}

@fragment
fn water_below(in: VertexOut) -> @location(0) vec4f {
    return shade_water(in.world_position, true);
}
