// Inspired by aaaa-zhen/glsl-playground liquid-glass/metaball-search (MIT).
// Logical coordinates. values[0] = center.xy, half-size.xy;
// values[1] = top edge (-1 disables), fusion, refraction, blur;
// values[2] = dark amount, chromatic aberration, lens zoom, corner radius.
fn round_box(p: vec2<f32>, c: vec2<f32>, b: vec2<f32>, r: f32) -> vec3<f32> {
    let lp = p-c;
    let q = abs(lp)-b+vec2<f32>(r);
    let mq = max(q, vec2<f32>(0.0));
    let len = length(mq);
    var grad = select(vec2<f32>(0.0, sign(lp.y)), vec2<f32>(sign(lp.x), 0.0), q.x>q.y);
    if len>0.0001 { grad = mq/len*sign(lp); }
    return vec3<f32>(len+min(max(q.x,q.y),0.0)-r, grad);
}
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let physical = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    let scale = max(plico.resolution_time_scale.w, 0.01);
    let size = physical/scale;
    let p = uv*size;
    let v = plico.values[0]*vec4<f32>(size,size);
    let opts = plico.values[1];
    let look = plico.values[2];
    let b = max(v.zw, vec2<f32>(1.0));
    var sdf = round_box(p, v.xy, b, clamp(look.w, 1.0, min(b.x,b.y)));
    if opts.x>=0.0 {
        let bar = vec3<f32>(p.y-opts.x, 0.0, 1.0);
        // Gradient-aware smooth union keeps aligned surfaces from inflating.
        let k = max(opts.y,0.0)*clamp(0.5-0.5*dot(bar.yz,sdf.yz),0.0,1.0)+0.0001;
        let h = clamp(0.5+0.5*(sdf.x-bar.x)/k,0.0,1.0);
        let d = mix(sdf.x,bar.x,h)-k*h*(1.0-h);
        sdf = vec3<f32>(d, normalize(mix(sdf.yz,bar.yz,h)+vec2<f32>(0.00001)));
    }
    let d = sdf.x;
    let aa = max(fwidth(d), 0.5/scale);
    let original = plico_backdrop_at(uv);
    if d>90.0 { return original; }
    let shadow = select(0.0, exp(-max(d,0.0)/18.0)*0.2, d>0.0);
    let outside = original.rgb*(1.0-shadow);
    let depth = clamp(-d/24.0,0.0,1.0);
    let mag = 1.0-sqrt(max(1.0-(1.0-depth)*(1.0-depth),0.0));
    let grad = normalize(sdf.yz+0.1*normalize(p-v.xy+vec2<f32>(0.00001)));
    let displacement = -clamp(opts.z,0.0,80.0)*mag*grad/size;
    let center = v.xy/size;
    let sample_uv = center+(uv+displacement-center)/clamp(look.z,1.0,2.0);
    let ab = displacement*clamp(look.y,0.0,0.5);
    let sigma = clamp(opts.w,0.0,32.0)*scale;
    let sharp = vec3<f32>(plico_backdrop_blur(sample_uv-ab,sigma).r,
        plico_backdrop_blur(sample_uv,sigma).g, plico_backdrop_blur(sample_uv+ab,sigma).b);
    let ny = (p.y-v.y)/b.y;
    var glass = mix(sharp, sharp*vec3<f32>(0.94,0.96,1.0)+vec3<f32>(0.01,0.012,0.02),0.22);
    let black = 1.0-smoothstep(-0.25,0.35,ny);
    glass = mix(glass,vec3<f32>(0.012,0.015,0.025),clamp(look.x,0.0,1.0)*black);
    if plico.values[3].x>0.5 {
        glass = sharp*clamp(0.10+0.12*ny,0.03,0.24)+vec3<f32>(0.008,0.010,0.014);
    }
    if plico.values[3].y>0.001 {
        let wave_uv = (p-v.xy+displacement*size*0.5)/(b*2.0)+vec2<f32>(0.5);
        let wave = plico_voice_color(wave_uv, b*2.0, plico.resolution_time_scale.z,0.65);
        glass += wave*clamp(plico.values[3].y,0.0,1.0);
    }
    let shell = 1.0-smoothstep(0.4,1.8,abs(d+1.5));
    let lit = pow(abs(dot(grad,vec2<f32>(0.7071))),8.0);
    glass += vec3<f32>(shell*(0.08+0.55*lit));
    let equator = exp(-pow(ny/0.055,2.0))*look.x*(1.0-clamp(plico.values[3].x,0.0,1.0));
    glass += vec3<f32>(0.18,0.22,0.3)*equator;
    return mix(vec4<f32>(outside,original.a),vec4<f32>(clamp(glass,vec3<f32>(0.0),vec3<f32>(1.0)),1.0),1.0-smoothstep(-aa,aa,d));
}
