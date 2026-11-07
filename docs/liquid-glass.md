# Liquid Glass

Plico owns the shared WGSL asset and the C# composition API. Applications own
appearance preferences; no password-manager or account code lives in this module.

```csharp
using (ui.LiquidGlass(new LiquidGlassOptions {
    Frost = .8f, Tint = .45f, Radius = 20,
    Refraction = 2, Animated = false,
}).Width(300).Padding(24).Gap(12)) {
    ui.Text("Sharp content over a refracted scene");
    ui.Button("Continue").Primary();
}
```

`horizontal: true` creates a row. `Enabled = false` produces an opaque surface
without a shader; `ReduceMotion = true` suppresses moving reflections. Animation
is off by default and uses the native clock without rebuilding the C# component.
The effect samples only content already painted behind the pane. Paint a background
first; it cannot read the desktop, another window or content painted afterwards.

Parameters are validated before declaring the pane. Frost/Tint/Highlight are 0–1,
Refraction is 0–16 logical pixels, Saturation is 0–2 and Radius is 0–256 logical
pixels. TintColor is RGB. Children remain normal sharp, interactive widgets.

The authoritative asset is `crates/plico-renderer/assets/liquid_glass.wgsl`.
It is embedded in SunDoge.Plico and exposed to Rust as
`plico_renderer::shader::LIQUID_GLASS_WGSL`. Advanced callers may use
`LiquidGlassEffect.Program` and `LiquidGlassEffect.Configure` on a shader surface.
For Rust/custom effect clients, enable backdrop input and provide these vec4 slots:

| Slot | x | y | z | w |
| --- | --- | --- | --- | --- |
| 0 | radius | frost | tint amount | reflection motion multiplier (0/1) |
| 1 | refraction | saturation | highlight strength | reserved (0) |
| 2 | tint red | tint green | tint blue | reserved (0) |

This is a wgpu scene effect, not an OS compositor API or a SkSL implementation.
Backdrop readers currently require conservative full damage. Prefer a few chrome
panes to hundreds of independently sampled list cells. Axis-aligned local regions
are covered by the offset-region GPU regression test; arbitrary clip masks,
transformed panes and partially offscreen panes still need additional coverage.
Do not infer universal clipping or power-efficiency guarantees from this demo.

The C# gallery's shader page demonstrates controls and an interactive pane. Pure
managed checks cover composition/fallback/validation without loading Rust; native
application checks cover shader validation and native-only animated frames.
