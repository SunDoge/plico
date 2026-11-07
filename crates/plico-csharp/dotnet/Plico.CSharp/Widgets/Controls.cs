using System.Runtime.CompilerServices;

namespace Plico;

// Widget composition and policy live in managed code, above the primitive ABI.
public sealed unsafe partial class Ui
{
    public Builder Button(string text, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        int index = Add(16, text, file, line, key);
        ref var c = ref commands[index];
        c.flags = (1U << 21) | 4U | 8U | (1U << 22) | (1U << 23);
        c.flags |= (1U << 9) | (1U << 11);
        c.height = 36; c.padding = 8; c.radius = 6; c.gap = 6; c.shrink = 0;
        c.background = Theme.Control;
        c.foreground = Theme.Foreground;
        c.hover_background = Theme.Hover;
        c.pressed_background = Theme.Pressed;
        c.border_width = 1; c.border_color = Theme.Border;
        var labelSpan = (c.text_start, c.text_len);
        parents.Push(index);
        try
        {
            if (text.Length > 0)
            {
                int label = Add(3, "", file, line, "label");
                commands[label].text_start = labelSpan.text_start;
                commands[label].text_len = labelSpan.text_len;
                commands[label].shrink = 0;
            }
        }
        finally { parents.Pop(); }
        return new Builder(this, index);
    }
    private const string CheckmarkSvg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'><path d='m3.25 8.25 3 3 6.5-6.5' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'/></svg>";
    public Builder Checkbox(string label, ref bool value, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) => Toggle(label, ref value, false, key, file, line);
    public Builder Svg(string source, float size = 18, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) =>
        Leaf(11, source, file, line, key).Width(size).Height(size).Shrink();
    public Builder Switch(string label, ref bool value, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) => Toggle(label, ref value, true, key, file, line);
    private Builder Toggle(string label, ref bool value, bool sliding, string? key, string file, int line)
    {
        var b = Button("", key, file, line).Ghost().Padding(0).JustifyStart();
        if (events.TryGetValue(b.Id, out var e) && e.Kind == 1) value = !value;
        ref var c = ref commands[b.Index];
        c.semantic_role = sliding ? 2U : 1U;
        if (value) c.flags |= 1U << 24;
        c.flags &= ~((1U << 22) | (1U << 23));
        var name = Encode(label); c.text_start = name.Item1; c.text_len = name.Item2;
        parents.Push(b.Index);
        try
        {
            using (var mark = Box(key: "mark", file: file, line: line).Width(sliding ? 34 : 18).Height(sliding ? 20 : 18).Shrink()
                .Radius(sliding ? 10 : 4).AlignCenter().Padding(sliding ? 2 : 0)
                .Background(value ? Theme.Accent : Theme.Border))
            {
                if (sliding)
                {
                    if (value) mark.JustifyEnd();
                    using (Box(key: "knob", file: file, line: line).Width(16).Height(16).Radius(8).Background(Theme.OnAccent)) { }
                }
                else
                {
                    mark.JustifyCenter();
                    if (value) Svg(CheckmarkSvg, 14, key: "check", file: file, line: line).Color(Theme.OnAccent);
                }
            }
            Text(label, key: "label", file: file, line: line).Shrink();
        }
        finally { parents.Pop(); }
        return b;
    }
    public Builder Slider(string label, ref double value, double min, double max, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!double.IsFinite(min) || !double.IsFinite(max) || max < min || !double.IsFinite(value))
            throw new ArgumentOutOfRangeException(nameof(max));
        int root = Add(16, "", file, line, key);
        commands[root].width = 200; commands[root].height = 32;
        commands[root].flags |= 1U << 9;
        parents.Push(root);
        try
        {
            var input = Leaf(19, label, file, line, "input");
            if (events.TryGetValue(input.Id, out var e) && e.Kind == 3) value = e.Value;
            value = Math.Clamp(value, min, max);
            commands[input.Index].value = value; commands[input.Index].min = min; commands[input.Index].max = max;
            float fraction = (float)(max == min ? 0 : (value - min) / (max - min));
            var accent = Theme.Accent;
            Shader(SliderEffect, key: "paint", file: file, line: line).ShaderValues(fraction, ((accent >> 16) & 255) / 255f, ((accent >> 8) & 255) / 255f, (accent & 255) / 255f).FillWidth().FillHeight();
        }
        finally { parents.Pop(); }
        return new Builder(this, root);
    }
    private const string SliderEffect = """
        fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
            let fraction = plico.values[0].x;
            let size = plico.resolution_time_scale.xy;
            let p = uv * size;
            let x = 8.0 + fraction * max(0.0, size.x - 16.0);
            let track = 1.0 - smoothstep(1.0, 2.0, abs(p.y - size.y * 0.5));
            let in_track = step(8.0, p.x) * step(p.x, size.x - 8.0);
            let knob = 1.0 - smoothstep(6.0, 7.0, length(p - vec2<f32>(x, size.y * 0.5)));
            let accent = plico.values[0].yzw;
            let color = mix(vec3<f32>(0.83), accent, step(p.x, x));
            let alpha = max(track * in_track, knob);
            return vec4<f32>(mix(color, accent, knob) * alpha, alpha);
        }
        """;
    public Builder Spinner(string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        uint accent = Theme.Accent;
        var b = Shader(SpinnerEffect, animated: true, key, file, line)
            .ShaderValues(((accent >> 16) & 255) / 255f, ((accent >> 8) & 255) / 255f, (accent & 255) / 255f).Width(20).Height(20);
        commands[b.Index].semantic_role = 6;
        return b;
    }
    private const string SpinnerEffect = """
        fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
            let p = uv - vec2<f32>(0.5);
            let radius = length(p);
            let ring = smoothstep(0.29, 0.33, radius) * (1.0 - smoothstep(0.41, 0.45, radius));
            let phase = atan2(p.y, p.x) - plico.resolution_time_scale.z * 5.0;
            let alpha = (0.2 + 0.8 * fract(phase / 6.2831853 + 1.0)) * ring;
            return vec4<f32>(plico.values[0].xyz * alpha, alpha);
        }
        """;

}
