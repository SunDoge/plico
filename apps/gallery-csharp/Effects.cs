using Plico;
namespace Gallery;

internal sealed partial class GalleryModel
{
    internal bool AnimateEffects = true;
    internal int EffectIndex;
    internal static readonly string[] EffectNames = ["Aurora", "Orb", "Ripple"];
    internal static readonly string[] EffectSources = [
        """
        fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
            let t = plico.resolution_time_scale.z;
            let wave = 0.5 + 0.5 * sin(uv.x * 5.0 - t * 0.45);
            let ribbon = exp(-abs(uv.y - 0.48 - 0.17 * sin(uv.x * 6.0 + t * 0.7)) * 12.0);
            let ribbon2 = exp(-abs(uv.y - 0.64 - 0.15 * sin(uv.x * 5.0 - t * 0.5)) * 14.0);
            let base = mix(vec3<f32>(0.025, 0.04, 0.12), vec3<f32>(0.05, 0.12, 0.23), uv.y);
            let light = mix(vec3<f32>(0.12, 0.42, 0.95), vec3<f32>(0.2, 0.85, 0.65), wave);
            return vec4<f32>(base + light * ribbon * 0.8 + vec3<f32>(0.33, 0.2, 0.65) * ribbon2, 1.0);
        }
        """,
        """
        fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
            let t = plico.resolution_time_scale.z;
            let p = (uv - vec2<f32>(0.5)) * vec2<f32>(1.6, 1.0);
            let radius = length(p);
            let halo = exp(-radius * radius * 15.0);
            let ring = exp(-abs(radius - 0.28) * 65.0);
            let swirl = 0.5 + 0.5 * sin(atan2(p.y, p.x) * 3.0 + radius * 18.0 - t);
            let color = mix(vec3<f32>(0.15, 0.45, 0.92), vec3<f32>(0.9, 0.35, 0.55), swirl);
            return vec4<f32>(vec3<f32>(0.025, 0.035, 0.07) + color * (halo * 0.65 + ring * 0.5), 1.0);
        }
        """,
        """
        fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
            let t = plico.resolution_time_scale.z;
            let d = length(uv - vec2<f32>(0.5));
            let rings = 0.5 + 0.5 * sin(d * 42.0 - t * 2.0);
            let fade = exp(-d * 3.0);
            let color = mix(vec3<f32>(0.08, 0.15, 0.3), vec3<f32>(0.32, 0.65, 0.85), rings * fade);
            return vec4<f32>(color, 1.0);
        }
        """
    ];
    private void Effects(Ui ui)
    {
        using (Card(ui, Tr("Shader studio", "着色器工作室")))
        {
            using (ui.Row().FillWidth().Gap(10).Wrap().AlignCenter())
            {
                for (int i = 0; i < EffectNames.Length; i++)
                    using (ui.Scope(i))
                        if ((EffectIndex == i ? ui.Button(EffectNames[i]).Primary() : ui.Button(EffectNames[i]).Ghost()).Clicked()) EffectIndex = i;
                ui.Switch("Animate", ref AnimateEffects);
            }
            ui.Shader(EffectSources[EffectIndex], animated: AnimateEffects, key: EffectNames[EffectIndex]).FillWidth().Height(330).Radius(12);
            Hint(ui, "The injected native clock drives this surface. Animation does not rebuild the C# page.");
        }
        using (Card(ui, "WGSL entry point"))
        {
            ui.Text("fn plico_effect(uv: vec2<f32>) -> vec4<f32>").Color(T.Accent);
            ui.Text("Time: plico.resolution_time_scale.z").Color(T.Muted);
            ui.Text("The effect returns premultiplied RGBA. This gallery uses opaque output.").Color(T.Muted).FillWidth();
        }
    }
}
