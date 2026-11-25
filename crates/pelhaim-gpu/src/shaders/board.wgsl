struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) half_size: vec2<f32>,
    @location(4) aa: f32,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local: vec2<f32>,
    @location(2) @interpolate(flat) half_size: vec2<f32>,
    @location(3) @interpolate(flat) aa: f32,
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    let ndc = v.pos / globals.viewport * 2.0 - 1.0;
    out.clip = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.color = v.color;
    out.local = v.local;
    out.half_size = v.half_size;
    out.aa = v.aa;
    return out;
}

fn sd_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    // Derivatives must be taken in uniform control flow, so coverage is computed for every
    // fragment and only selected by `aa` afterwards.
    let fw = max(fwidth(v.local.x), 1e-4);
    let sdf = clamp(0.5 - sd_box(v.local, v.half_size) / fw, 0.0, 1.0);
    let coverage = select(1.0, sdf, v.aa > 0.5);
    return v.color * coverage;
}
