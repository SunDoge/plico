using System.Globalization;
using System.Runtime.CompilerServices;

namespace Plico;

public enum Tone { Neutral, Info, Success, Warning, Danger }

public sealed unsafe partial class Ui
{
    private uint ToneColor(Tone tone) => tone switch
    {
        Tone.Info => Theme.Info,
        Tone.Success => Theme.Success,
        Tone.Warning => Theme.Warning,
        Tone.Danger => Theme.Danger,
        _ => Theme.Muted
    };

    /// A themed content surface. Dispose after declaring its children.
    public Container Card(string? title = null, string? description = null, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var card = Column(key, file, line).FillWidth().Shrink().Padding(20).Gap(12)
            .Radius(10).Background(Theme.Panel).Border(1, Theme.Border);
        if (!string.IsNullOrEmpty(title)) Text(title, key: "title", file: file, line: line).FontSize(16).Weight(650).FillWidth();
        if (!string.IsNullOrEmpty(description)) Text(description, key: "description", file: file, line: line).Color(Theme.Muted).FillWidth();
        return card;
    }

    /// Vertical field with a visible label and optional help/error. Input controls
    /// still need their own accessible label; this is not a native label association.
    public Container Field(string label, string? help = null, string? error = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var field = Column(file: file, line: line).FillWidth().Shrink().Gap(6);
        Text(label, key: "label", file: file, line: line).Weight(600);
        string? note = string.IsNullOrEmpty(error) ? help : error;
        if (!string.IsNullOrEmpty(note)) Text(note, key: "note", file: file, line: line).Color(string.IsNullOrEmpty(error) ? Theme.Muted : Theme.Danger).FillWidth();
        return field;
    }

    public void Divider([CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using (Box(file: file, line: line).FillWidth().Height(1).Shrink().Background(Theme.Border)) { }
    }

    public void Badge(string label, Tone tone = Tone.Neutral,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using (Row(file: file, line: line).PaddingX(8).PaddingY(4).Radius(5).Shrink().Background(Theme.Control).Border(1, Theme.Border))
            Text(label, key: "label", file: file, line: line).Color(ToneColor(tone)).Weight(600);
    }

    public void Avatar(string name, ulong image = 0, float size = 36,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!float.IsFinite(size) || size < 16 || size > 512) throw new ArgumentOutOfRangeException(nameof(size));
        ArgumentNullException.ThrowIfNull(name);
        using (Box(file: file, line: line).Width(size).Height(size).Shrink().Radius(size / 2).Clip()
            .Background(Theme.Selection).AlignCenter().JustifyCenter())
        {
            if (image != 0) Image(image, size, size, key: "image", file: file, line: line);
            else
            {
                // Text elements preserve combining marks and surrogate pairs.
                string[] words = name.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);
                string initials = words.Length == 0 ? "?" : StringInfo.GetNextTextElement(words[0]);
                if (words.Length > 1) initials += StringInfo.GetNextTextElement(words[^1]);
                Text(initials.ToUpperInvariant(), key: "initials", file: file, line: line).Weight(650)
                    .FontSize((uint)Math.Max(12, size * .38f)).Color(Theme.Info);
            }
        }
    }

    public void Alert(string title, string message, Tone tone = Tone.Info,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using (Row(file: file, line: line).FillWidth().Shrink().Gap(12).Padding(14).Radius(8).Background(Theme.Control).Border(1, Theme.Border).AlignStart())
        {
            Svg("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.7'><circle cx='12' cy='12' r='9'/><path d='M12 10v7M12 6v2'/></svg>", 20, key: "icon", file: file, line: line).Color(ToneColor(tone));
            using (Column(key: "content", file: file, line: line).Width(0).Grow().MinWidth(0).Gap(5))
            {
                Text(title, key: "title", file: file, line: line).Weight(650).FillWidth();
                Text(message, key: "message", file: file, line: line).Color(Theme.Muted).FillWidth();
            }
        }
    }

    /// Returns true when the optional action is activated. No IO is performed.
    public bool EmptyState(string title, string description, string? action = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using (Column(file: file, line: line).FillWidth().Padding(28).Gap(12).AlignCenter())
        {
            Text(title, key: "title", file: file, line: line).FontSize(18).Weight(650);
            Text(description, key: "description", file: file, line: line).FillWidth().Color(Theme.Muted);
            return !string.IsNullOrEmpty(action) && Button(action, key: "action", file: file, line: line).Primary().Clicked();
        }
    }

    public Builder Skeleton(float width = 160, float height = 16, bool animated = true,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!float.IsFinite(width) || !float.IsFinite(height) || width <= 0 || height <= 0 || width > 16384 || height > 16384) throw new ArgumentOutOfRangeException(nameof(width));
        uint color = Theme.Border;
        return Shader(SkeletonEffect, animated, file: file, line: line).Width(width).Height(height).Radius(4)
            .ShaderValues(((color >> 16) & 255) / 255f, ((color >> 8) & 255) / 255f, (color & 255) / 255f, animated ? 1 : 0);
    }
    private const string SkeletonEffect = """
        fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
            let base = plico.values[0].xyz;
            let phase = uv.x - fract(plico.resolution_time_scale.z * 0.45) * 2.0 + 0.5;
            let shine = exp(-phase * phase * 18.0) * 0.09 * plico.values[0].w;
            return vec4<f32>(base + vec3<f32>(shine), 1.0);
        }
        """;
}
