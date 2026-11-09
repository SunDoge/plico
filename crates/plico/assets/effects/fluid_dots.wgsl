// WGSL translation of aaaa-zhen/glsl-playground siri-wave.html (MIT).
const TAU: f32 = 6.28318530718;
const N: i32 = 6;
const SMOOTH_K: f32 = 0.08;
const INTENSITY: f32 = 0.0025;
const FALLOFF_P: f32 = 1.35;
const FADE_START: f32 = 0.02;
const FADE_END: f32 = 0.56;
const ABERR: f32 = 0.005;
const SPECTRAL: vec3<f32> = vec3<f32>(0.0, 0.5, 1.0) * ABERR;
const HUE_SPEED: f32 = 0.06;
const COLOR_K: f32 = 0.5;
const SAT: f32 = 0.01;
const HUE_SPAN: f32 = 0.667;
const MERGE_PERIOD: f32 = 6.0;
const T_MOVE: f32 = 1.25;
const STAGGER: f32 = 0.33;
const HOLD: f32 = 0.0;
const W: f32 = 4.6;
const L: f32 = 3.2;
const PIERCE: f32 = 0.12;
const RECOIL: f32 = 0.035;
const REC_LAG: f32 = 0.11;
const GATHER_PERIOD: f32 = 12.0;
const GATHER_START: f32 = 9.2;
const GATHER_HOLD: f32 = 0.8;
const GATHER_R: f32 = 0.008;
const GATHER_DIM: f32 = 0.85;
const GATHER_IN: f32 = 1.8;
const GATHER_IN_L: f32 = 7.5;
const BURST_W: f32 = 6.5;
const BURST_L: f32 = 4.0;
const CHARGE_T: f32 = 0.30;
const CHARGE_SHRK: f32 = 0.18;
const CHARGE_GLOW: f32 = 0.35;
const FLASH_GAIN: f32 = 1.2;
const FLASH_DECAY: f32 = 7.0;

fn hash11(n: f32) -> f32 {
    return fract(sin(n * 127.1 + 311.7) * 43758.5453);
}
fn settleWL(tau: f32, w: f32, l: f32) -> f32 {
    if tau <= 0.0 {
        return 0.0;
    }
    return 1.0 - exp(-l * tau) * cos(w * tau);
}
fn settle(tau: f32) -> f32 {
    return settleWL(tau, W, L);
}
fn settleCrit(tau: f32, l: f32) -> f32 {
    if tau <= 0.0 {
        return 0.0;
    }
    return 1.0 - exp(-l * tau) * (1.0 + l * tau);
}
fn smin(a: f32, b: f32, k: f32) -> f32 {
    var h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}
fn hue2rgb(h: f32) -> vec3<f32> {
    let hue = fract(h);
    var r = clamp(abs(hue * 6.0 - 3.0) - 1.0, 0.0, 1.0);
    var g = clamp(2.0 - abs(hue * 6.0 - 2.0), 0.0, 1.0);
    var b = clamp(2.0 - abs(hue * 6.0 - 4.0), 0.0, 1.0);
    return vec3<f32>(r, g, b);
}
fn dotR(fi: f32, seed: f32, t: f32) -> f32 {
    return 0.036 + 0.010 * sin(t * 1.3 + seed * TAU) + 0.005 * sin(t * 2.4 + fi * 1.3);
}
fn dotSD(p: vec2<f32>, pos: vec2<f32>, r: f32, t: f32, fi: f32, shapeDamp: f32) -> f32 {
    var d = p - pos;
    var sq = 0.075 * (0.5 + 0.5 * sin(t * 0.9 + fi * 2.0)) * shapeDamp;
    var ca = cos(t * 0.35 + fi);
    var sa = sin(t * 0.35 + fi);
    d = mat2x2<f32>(
            ca, -sa,
            sa, ca,
        ) * d;
    d *= vec2<f32>(1.0 + sq, 1.0 - sq);
    return length(d) - r;
}
fn scene(p: vec2<f32>, t: f32) -> vec3<f32> {
    var k = floor(t / MERGE_PERIOD);
    var u = fract(t / MERGE_PERIOD);
    var te = u * MERGE_PERIOD;
    var tg = (t % GATHER_PERIOD);
    var g = settleCrit((tg - GATHER_START) * GATHER_IN, GATHER_IN_L)
        - settleWL(tg - GATHER_START - GATHER_HOLD, BURST_W, BURST_L);
    var gC = clamp(g, 0.0, 1.0);
    var tb = tg - (GATHER_START + GATHER_HOLD);
    var charge = smoothstep(-CHARGE_T, 0.0, min(tb, 0.0)) * gC;
    var flash = select(0.0, exp(-max(tb, 0.0) * FLASH_DECAY), tb > 0.0);
    var gBright = mix(1.0, GATHER_DIM, gC) * (1.0 + CHARGE_GLOW * charge + FLASH_GAIN * flash);
    var total3 = vec3<f32>(1e5);
    var cAcc = vec3<f32>(0.0);
    var wAcc = 1e-6;
    for(var i = 0; i < N; i++) {
        var fi = f32(i);
        var seed = hash11(fi);
        var ang = fi / f32(N) * TAU + t * 0.35;
        var dir = vec2<f32>(cos(ang), sin(ang));
        var R = 0.17 + 0.010 * sin(t * 1.0) + 0.007 * sin(t * 1.3 + seed * TAU);
        var pairId = (fi % 3.0);
        var moverLow = ((k + pairId) % 2.0);
        var isMover = select(step(0.5, moverLow), step(moverLow, 0.5), fi < 2.5);
        var goStart = pairId * STAGGER;
        var retStart = 3.0 * STAGGER + HOLD + pairId * STAGGER;
        var m = (settle(te - goStart) - settle(te - retStart)) * isMover;
        var rec = (settle(te - goStart - REC_LAG) - settle(te - retStart - REC_LAG)) * (1.0 - isMover);
        var rSelf = dotR(fi, seed, t);
        rSelf = mix(rSelf, 0.036, gC);
        rSelf *= 1.0 - CHARGE_SHRK * charge;
        var fj = ((fi + 3.0) % 6.0);
        var rPart = dotR(fj, hash11(fj), t);
        var deep = -(R + RECOIL) - PIERCE * rPart;
        var radial = mix(R, deep, m) + RECOIL * rec;
        radial = mix(radial, GATHER_R, g);
        var pos = radial * dir;
        var sdR = dotSD(p - SPECTRAL.r * dir, pos, rSelf, t, fi, 1.0 - gC);
        var sdG = dotSD(p - SPECTRAL.g * dir, pos, rSelf, t, fi, 1.0 - gC);
        var sdB = dotSD(p - SPECTRAL.b * dir, pos, rSelf, t, fi, 1.0 - gC);
        total3 = vec3<f32>(smin(total3.r, sdR, SMOOTH_K), smin(total3.g, sdG, SMOOTH_K), smin(total3.b, sdB, SMOOTH_K));
        var hue = fract(fi / f32(N) + t * HUE_SPEED) * HUE_SPAN;
        var dotCol = mix(vec3<f32>(1.0), hue2rgb(hue), SAT);
        var w = exp(-sdG * COLOR_K);
        cAcc += w * dotCol;
        wAcc += w;
    }
    var sd3 = max(total3, vec3<f32>(0.0)) + 1e-4;
    var core3 = clamp(vec3<f32>(INTENSITY) / pow(sd3, vec3<f32>(FALLOFF_P)), vec3<f32>(0.0), vec3<f32>(1.0));
    var edge3 = vec3<f32>(1.0) - smoothstep(vec3<f32>(FADE_START), vec3<f32>(FADE_END), sd3);
    var bright = core3 * edge3 * gBright;
    return bright * (cAcc / wAcc);
}

fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let res = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    let t = plico.resolution_time_scale.z;
    let p = (uv * 2.0 - 1.0) * vec2<f32>(res.x, -res.y) / min(res.x, res.y) / (1.0 + 0.03 * sin(t));
    let col = scene(p, t) * (1.0 + 0.05 * sin(t + 1.0));
    return vec4<f32>(min(pow(max(col, vec3<f32>(0.0)), vec3<f32>(1.0 / 1.2)), vec3<f32>(1.0)), 1.0);
}
