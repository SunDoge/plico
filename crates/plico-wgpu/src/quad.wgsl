struct Frame {
    size_time_scale: vec4f,
    background: vec4f,
    origin: vec4f,
}
@group(0) @binding(0)
var<uniform> frame: Frame;
@group(1) @binding(0)
var glyph_atlas: texture_2d<f32>;
@group(1) @binding(1)
var glyph_sampler: sampler;
@group(2) @binding(0)
var clip_mask: texture_2d<f32>;
@group(2) @binding(1)
var clip_sampler: sampler;
// PLICO_INTERFACE
// PLICO_MATERIAL
struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) local: vec2f,
    @interpolate(flat)
    @location(1) size: vec2f,
    @interpolate(flat)
    @location(2) color: vec4f,
    @interpolate(flat)
    @location(3) color2: vec4f,
    @interpolate(flat)
    @location(4) border: vec4f,
    @interpolate(flat)
    @location(5) config: vec4f,
    @interpolate(flat)
    @location(6) clip: vec4f,
    @location(7) uv: vec2f,
    @interpolate(flat)
    @location(8) gradient: vec4f,
    @interpolate(flat)
    @location(9) clip_params: vec4f,
    @interpolate(flat)
    @location(10) mask_rect: vec4f,
    @interpolate(flat)
    @location(11) radii: vec4f,
}
@vertex
fn vs_main(
    @builtin(vertex_index) index: u32,
    @location(0) rect: vec4f,
    @location(1) color: vec4f,
    @location(2) color2: vec4f,
    @location(3) border: vec4f,
    @location(4) config: vec4f,
    @location(5) clip: vec4f,
    @location(6) uv: vec4f,
    @location(7) gradient: vec4f,
    @location(8) clip_params: vec4f,
    @location(9) mask_rect: vec4f,
    @location(10) radii: vec4f,
) -> Vertex {
    let corners = array<vec2f, 6>(vec2f(0, 0), vec2f(1, 0), vec2f(0, 1), vec2f(0, 1), vec2f(1, 0), vec2f(1, 1));
    // Include the 1px antialiasing fringe, rather than clipping it at the quad.
    let local = corners[index] * (rect.zw + 2.) - 1.;
    let world = rect.xy + local;
    var out: Vertex;
    out.position = vec4f((world - frame.origin.xy) / frame.size_time_scale.xy * vec2f(2, -2) + vec2f(-1, 1), 0, 1);
    out.local = local;
    out.size = rect.zw;
    out.color = color;
    out.color2 = color2;
    out.clip_params = clip_params;
    out.mask_rect = mask_rect;
    out.radii = radii;
    out.gradient = gradient;
    out.uv = uv.xy + local / rect.zw * uv.zw;
    if config.z == 4. && clip_params.y > 0.5 {
        out.uv = vec2f(uv.x * world.x + uv.z * world.y, uv.y * world.x + uv.w * world.y) + gradient.zw;
    }
    out.border = border;
    out.config = config;
    out.clip = clip;
    return out;
}
// Per-corner radius: top-left, top-right, bottom-right, bottom-left, selected
// by the point's quadrant. The CPU shrinks overlapping adjacent corners first.
fn corner_radius(p: vec2f, size: vec2f, radii: vec4f) -> f32 {
    let top = select(radii.x, radii.y, p.x >= size.x * 0.5);
    let bottom = select(radii.w, radii.z, p.x >= size.x * 0.5);
    return select(bottom, top, p.y < size.y * 0.5);
}
fn distance_to_round_rect(p: vec2f, size: vec2f, radii: vec4f) -> f32 {
    let r = clamp(corner_radius(p, size, radii), 0., min(size.x, size.y) * 0.5);
    let q = abs(p - size * 0.5) - size * 0.5 + r;
    return length(max(q, vec2f(0))) + min(max(q.x, q.y), 0.) - r;
}
fn coverage(p: vec2f, size: vec2f, radii: vec4f) -> f32 {
    if all(radii <= vec4f(0.)) {
        // Exact pixel-area coverage for an axis-aligned rectangle, including corners.
        let c = clamp(min(size, p + 0.5) - max(vec2f(0), p - 0.5), vec2f(0), vec2f(1));
        return c.x * c.y;
    }
    return clamp(0.5 - distance_to_round_rect(p, size, radii), 0., 1.);
}
// Normal CDF approximation (maximum absolute error below 8e-8).
fn normal_cdf(x: f32) -> f32 {
    let a = abs(x);
    let t = 1. / (1. + 0.2316419 * a);
    let tail = 0.39894228 * exp(-0.5 * a * a) * t
        * (0.31938153 + t * (-0.35656378 + t * (1.78147794 + t * (-1.82125598 + t * 1.33027443))));
    return select(tail, 1. - tail, x >= 0.);
}
// Integrate the horizontal Gaussian analytically, then use bounded midpoint
// quadrature vertically. Restrict integration to four sigma around this pixel.
fn shadow_coverage(p: vec2f, size: vec2f, radius: f32, sigma: f32) -> f32 {
    let r = clamp(radius, 0., min(size.x, size.y) * 0.5);
    if r <= 0. || (p.y - 4. * sigma >= r && p.y + 4. * sigma <= size.y - r) {
        return (normal_cdf((size.x - p.x) / sigma) - normal_cdf(-p.x / sigma))
            * (normal_cdf((size.y - p.y) / sigma) - normal_cdf(-p.y / sigma));
    }
    let lo = max(0., p.y - 4. * sigma);
    let hi = min(size.y, p.y + 4. * sigma);
    if hi <= lo {
        return 0.;
    }
    let step = (hi - lo) / 32.;
    var sum = 0.;
    for(var i = 0u; i < 32u; i++) {
        let y = lo + (f32(i) + 0.5) * step;
        let dy = max(abs(y - size.y * 0.5) - (size.y * 0.5 - r), 0.);
        let inset = r - sqrt(max(r * r - dy * dy, 0.));
        let horizontal = normal_cdf((size.x - inset - p.x) / sigma) - normal_cdf((inset - p.x) / sigma);
        let z = (y - p.y) / sigma;
        sum += horizontal * exp(-0.5 * z * z);
    }
    return clamp(sum * step * 0.39894228 / sigma, 0., 1.);
}
// Mitchell-Netravali reconstruction with B=C=1/3. Source texels are
// already premultiplied; clamp ringing back into valid premultiplied RGBA.
fn cubic_weight(value: f32) -> f32 {
    let x = abs(value);
    if x < 1. {
        return (7. * x * x * x - 12. * x * x + 16. / 3.) / 6.;
    }
    if x < 2. {
        return ((-7. / 3. * x + 12.) * x * x - 20. * x + 32. / 3.) / 6.;
    }
    return 0.;
}
fn image_coordinate(x: i32, size: i32, mode: f32) -> i32 {
    if mode < 0.5 {
        return clamp(x, 0, size - 1);
    }
    if mode < 1.5 {
        return ((x % size) + size) % size;
    }
    let p = ((x % (2 * size)) + 2 * size) % (2 * size);
    return min(p, 2 * size - 1 - p);
}
fn image_uv_axis(value: f32, mode: f32) -> f32 {
    if mode < 0.5 {
        return clamp(value, -2., 3.);
    }
    if mode < 1.5 {
        return fract(value);
    }
    return 1. - abs(2. * fract(value * 0.5) - 1.);
}
fn bicubic(uv: vec2f, extend: vec2f, level: i32) -> vec4f {
    let size = vec2i(textureDimensions(glyph_atlas, level));
    // Pad extension is constant beyond the reconstruction halo. Bounding UV
    // before integer conversion also avoids overflow for extreme brush offsets.
    let p = clamp(uv, vec2f(-2.), vec2f(3.)) * vec2f(size) - 0.5;
    let base = vec2i(floor(p));
    let fraction = fract(p);
    var result = vec4f(0.);
    for(var y = -1; y <= 2; y++) {
        for(var x = -1; x <= 2; x++) {
            let weight = cubic_weight(f32(x) - fraction.x) * cubic_weight(f32(y) - fraction.y);
            result += textureLoad(
                    glyph_atlas,
                    vec2i(
                        image_coordinate(base.x + x, size.x, extend.x),
                        image_coordinate(base.y + y, size.y, extend.y),
                    ),
                    level,
                ) * weight;
        }
    }
    let alpha = clamp(result.a, 0., 1.);
    return vec4f(clamp(result.rgb, vec3f(0.), vec3f(alpha)), alpha);
}
@fragment
fn fs_main(input: Vertex) -> @location(0) vec4f {
    // Derivatives precede clipping/discard and extension folding. This avoids
    // false LOD spikes at repeat seams and supports affine source mappings.
    let source_size = vec2f(textureDimensions(glyph_atlas, 0));
    let footprint = max(length(dpdx(input.uv) * source_size), length(dpdy(input.uv) * source_size));
    let lod = clamp(log2(max(footprint, 1.)), 0., max(input.color2.x - 1., 0.));
    let world = input.position.xy + frame.origin.xy;
    let clip = input.clip;
    // Clip rectangle is a hard scissor equivalent, in physical pixels.
    var clip_alpha = 1.;
    if input.clip_params.x > 0.5 {
        clip_alpha = coverage(world - clip.xy, clip.zw, vec4f(0.));
        if clip_alpha <= 0. {
            discard;
        }
    } else if any(world < clip.xy) || any(world >= clip.xy + clip.zw) {
        discard;
    }
    if input.clip_params.z > 0. {
        let m = input.mask_rect;
        if any(world < m.xy) || any(world >= m.xy + m.zw) {
            discard;
        }
        clip_alpha *= textureSampleLevel(clip_mask, clip_sampler, (world - m.xy) / m.zw, 0.).a;
        if clip_alpha <= 0. {
            discard;
        }
    }
    if input.config.z >= 5. {
        let alpha = input.color.a * shadow_coverage(
                input.local - vec2f(input.gradient.x),
                input.gradient.zw,
                input.config.x,
                input.gradient.y,
            ) * clip_alpha;
        return vec4f(input.color.rgb * alpha, alpha);
    }
    if input.config.z >= 4. {
        let extend = input.gradient.xy;
        let uv = vec2f(image_uv_axis(input.uv.x, extend.x), image_uv_axis(input.uv.y, extend.y));
        var pixel: vec4f;
        if input.clip_params.w > 0.5 {
            let lower = i32(floor(lod));
            let upper = min(lower + 1, i32(max(input.color2.x, 1.)) - 1);
            pixel = bicubic(uv, extend, lower);
            if upper != lower && fract(lod) > 0.0001 {
                pixel = mix(pixel, bicubic(uv, extend, upper), fract(lod));
            }
        } else {
            pixel = textureSampleLevel(glyph_atlas, glyph_sampler, uv, select(lod, 0., input.clip_params.w < -0.5));
        }
        return pixel * input.color.a * coverage(input.local, input.size, input.radii) * clip_alpha;
    }
    if input.config.z >= 2. {
        let glyph = textureSampleLevel(glyph_atlas, glyph_sampler, input.uv, 0.);
        if input.config.z >= 3. {
            return glyph * input.color.a * clip_alpha;
        }
        return vec4f(input.color.rgb * glyph.a * input.color.a, glyph.a * input.color.a) * clip_alpha;
    }
    let outer = coverage(input.local, input.size, input.radii);
    let width = min(input.config.y, min(input.size.x, input.size.y) * 0.5);
    let inner_size = max(input.size - 2. * width, vec2f(0));
    var inner = 0.;
    if all(inner_size > vec2f(0)) {
        inner = coverage(input.local - vec2f(width), inner_size, max(input.radii - vec4f(width), vec4f(0.)));
    }
    let border_coverage = max(outer - inner, 0.);
    let uv = input.local / input.size;
    let start = input.gradient.xy * input.size;
    let direction = (input.gradient.zw - input.gradient.xy) * input.size;
    let t = clamp(dot(input.local - start, direction) / max(dot(direction, direction), 0.000001), 0., 1.);
    var fill = mix(input.color, input.color2, t);
    if input.clip_params.y > 0.5 {
        let a = vec4f(input.color.rgb * input.color.a, input.color.a);
        let b = vec4f(input.color2.rgb * input.color2.a, input.color2.a);
        let premultiplied = mix(a, b, t);
        fill = vec4f(premultiplied.rgb / max(premultiplied.a, 0.000001), premultiplied.a);
    }
    if input.config.z > 0.5 {
        fill = plico_material(
                Material(uv, input.size, input.color, input.color2, frame.size_time_scale.z, input.config.w),
            );
    }
    let fill_coverage = select(outer, inner, width > 0.);
    let alpha = fill.a * fill_coverage;
    let border_alpha = input.border.a * border_coverage;
    return vec4f(fill.rgb * alpha + input.border.rgb * border_alpha, alpha + border_alpha) * clip_alpha;
}
