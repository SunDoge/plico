using System.Numerics;
using System.Runtime.CompilerServices;
namespace Plico;

/// App-local scene blur/refraction, independent of desktop compositor effects.
public readonly record struct LiquidGlassOptions
{
    public LiquidGlassOptions() { }
    public bool Enabled { get; init; } = true;
    public bool Animated { get; init; }
    public bool ReduceMotion { get; init; }
    public float Radius { get; init; } = 18;
    public float Frost { get; init; } = .78f;
    public float Tint { get; init; } = .48f;
    public uint TintColor { get; init; } = 0xf9fcff;
    public float Refraction { get; init; } = 2;
    public float Saturation { get; init; } = 1.24f;
    public float Highlight { get; init; } = 1;
    internal void Validate()
    {
        static void Range(float value, float max, string name)
        { if (!float.IsFinite(value) || value < 0 || value > max) throw new ArgumentOutOfRangeException(name); }
        Range(Radius, 256, nameof(Radius)); Range(Frost, 1, nameof(Frost)); Range(Tint, 1, nameof(Tint));
        Range(Refraction, 16, nameof(Refraction)); Range(Saturation, 2, nameof(Saturation)); Range(Highlight, 1, nameof(Highlight));
        if (TintColor > 0xffffff) throw new ArgumentOutOfRangeException(nameof(TintColor));
    }
}

public static class LiquidGlassEffect
{
    /// Also usable as a plain shader surface. Uniform packing is set by Configure.
    public static WgslProgram Program { get; } = WgslProgram.Embedded(typeof(Ui).Assembly, "Plico.liquid_glass.wgsl", WgslMode.Backdrop);
    public static Ui.Builder Configure(Ui.Builder surface, LiquidGlassOptions options)
    {
        options.Validate();
        uint color = options.TintColor;
        return surface.Uniform(0, new Vector4(options.Radius, options.Frost, options.Tint, options.Animated && !options.ReduceMotion ? 1 : 0))
            .Uniform(1, new Vector4(options.Refraction, options.Saturation, options.Highlight, 0))
            .Uniform(2, new Vector3((color >> 16) / 255f, ((color >> 8) & 255) / 255f, (color & 255) / 255f));
    }
}

public sealed unsafe partial class Ui
{
    /// A column (or row) whose background samples the already-painted scene.
    /// Children are normal sharp widgets. No OS blur or external desktop pixels.
    public Container LiquidGlass(LiquidGlassOptions? options = null, bool horizontal = false, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var value = options ?? new LiquidGlassOptions(); value.Validate();
        var pane = horizontal ? Row(key, file, line) : Column(key, file, line);
        if (value.Enabled)
            LiquidGlassEffect.Configure(Shader(LiquidGlassEffect.Program, value.Animated && !value.ReduceMotion, key: "glass-background"), value)
                .Absolute(0, 0).FillWidth().FillHeight();
        else pane.Background(value.TintColor).Border(1, Theme.Border).Radius(value.Radius);
        return pane;
    }
}
