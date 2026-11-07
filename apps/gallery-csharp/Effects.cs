using Plico;
namespace Gallery;

internal sealed partial class GalleryModel
{
    internal bool AnimateEffects = true;
    internal int EffectIndex;
    internal double EffectStrength = 1;
    internal static readonly string[] EffectNames = ["Aurora", "Orb", "Ripple"];
    private static readonly string[] EffectFiles = ["aurora.wgsl", "orb.wgsl", "ripple.wgsl"];
    private WgslFile[] effectPrograms = [];
    private void InitializeEffects()
    {
        string source = Path.Combine(Environment.CurrentDirectory, "apps", "gallery-csharp", "assets", "shaders");
        string directory = Directory.Exists(source) ? source : Path.Combine(AppContext.BaseDirectory, "assets", "shaders");
        effectPrograms = EffectFiles.Select(file => new WgslFile(Path.Combine(directory, file), App, watch: true)).ToArray();
    }
    private void Effects(Ui ui)
    {
        LiquidGlassDemo(ui);
        var asset = effectPrograms[EffectIndex];
        using (Card(ui, Tr("Shader studio", "着色器工作室")))
        {
            using (ui.Row().FillWidth().Gap(10).Wrap().AlignCenter())
            {
                for (int i = 0; i < EffectNames.Length; i++)
                    using (ui.Scope(i))
                        if ((EffectIndex == i ? ui.Button(EffectNames[i]).Primary() : ui.Button(EffectNames[i]).Ghost()).Clicked()) EffectIndex = i;
                ui.Switch("Animate", ref AnimateEffects);
            }
            asset = effectPrograms[EffectIndex];
            ui.Slider("Effect strength", ref EffectStrength, .1, 1.5).Width(240);
            if (asset.Current is { } program)
                ui.Shader(program, animated: AnimateEffects, key: EffectNames[EffectIndex])
                    .Uniform(0, (float)EffectStrength).FillWidth().Height(330).Radius(12);
            if (asset.Error is { } error)
                using (ui.Column().FillWidth().Height(150).ScrollY().Gap(8))
                {
                    ui.Text("WGSL error · previous valid program retained").Weight(650).Color(ui.Theme.Danger);
                    ui.Text(error).FillWidth().Color(T.Muted);
                }
            Hint(ui, $"{Path.GetFileName(asset.Path)} · revision {asset.Revision} · live file reload");
            Hint(ui, "Edit assets/shaders/*.wgsl. No C# or Rust rebuild; the last good program stays visible on errors.");
        }
        using (Card(ui, "WGSL entry point"))
        {
            ui.Text("fn plico_effect(uv: vec2<f32>) -> vec4<f32>").Color(T.Accent);
            ui.Text("Time: plico.resolution_time_scale.z · parameters: plico.values[0].x").Color(T.Muted);
            ui.Text("Uniform slot 0 is supplied by Uniform(0, strength). Native animation frames do not rebuild this C# page.").Color(T.Muted).FillWidth();
        }
    }
}

internal sealed partial class GalleryModel
{
    private bool liquidEnabled = true, liquidAnimated, liquidReduceMotion;
    private double liquidFrost = .18, liquidTint = .16, liquidRefraction = 28;
    private static readonly WgslProgram GlassBackdrop = new("""
        fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
            let stripe = 0.5 + 0.5 * sin((uv.x + uv.y * 0.45) * 34.0);
            let color = mix(vec3<f32>(0.06, 0.24, 0.45), vec3<f32>(0.16, 0.72, 0.75), smoothstep(0.15, 0.85, stripe));
            let line = 1.0 - smoothstep(0.015, 0.04, abs(fract(uv.x * 12.0) - 0.5));
            return vec4<f32>(color + vec3<f32>(line * 0.14), 1.0);
        }
        """, "gallery-glass-backdrop.wgsl");
    private void LiquidGlassDemo(Ui ui)
    {
        using (Card(ui, Tr("Liquid Glass", "液态玻璃")))
        {
            using (ui.Row().FillWidth().Gap(16).Wrap())
            {
                ui.Switch("Liquid Glass", ref liquidEnabled);
                ui.Switch("Moving reflections", ref liquidAnimated);
                ui.Switch("Reduce glass motion", ref liquidReduceMotion);
            }
            using (ui.Row().FillWidth().Gap(16).Wrap())
            {
                using (ui.Column().Gap(6)) { ui.Text($"Frost · {liquidFrost:P0}").Color(T.Muted); ui.Slider("Glass frost", ref liquidFrost, 0, 1).Width(240); }
                using (ui.Column().Gap(6)) { ui.Text($"Tint · {liquidTint:P0}").Color(T.Muted); ui.Slider("Glass tint", ref liquidTint, 0, 1).Width(240); }
                using (ui.Column().Gap(6)) { ui.Text($"Refraction · {liquidRefraction:0} px").Color(T.Muted); ui.Slider("Glass refraction", ref liquidRefraction, 0, 64).Width(240); }
            }
            using (ui.Box().FillWidth().Height(240).Radius(16).Background(0xd0e9f2))
            {
                ui.Shader(GlassBackdrop).Absolute(0, 0).FillWidth().FillHeight();
                using (ui.LiquidGlass(new LiquidGlassOptions {
                    Enabled = liquidEnabled, Animated = liquidAnimated, ReduceMotion = liquidReduceMotion,
                    Frost = (float)liquidFrost, Tint = (float)liquidTint, Refraction = (float)liquidRefraction,
                }).Absolute(24, 32).Width(300).Height(176).Padding(24).Gap(14))
                {
                    ui.Text(Tr("A window into your scene", "映入场景的玻璃")).FontSize(20).Weight(650).Color(liquidEnabled ? 0xffffffU : ui.Theme.Foreground).FillWidth();
                    ui.Text(Tr("Blurred backdrop. Sharp text. Native WGSL animation.", "背景模糊，文字清晰，WGSL 驱动动画。")).Color(liquidEnabled ? 0xe4f4ffU : ui.Theme.Muted).FillWidth();
                    ui.Button(Tr("Still interactive", "依然可以交互")).Primary();
                }
            }
            Hint(ui, "ui.LiquidGlass(options) · app-local backdrop · opaque fallback · animations off by default");
        }
    }
}
