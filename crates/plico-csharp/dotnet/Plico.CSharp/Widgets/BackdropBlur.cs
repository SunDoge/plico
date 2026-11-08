using System.Numerics;
using System.Runtime.CompilerServices;
namespace Plico;

public enum ScrollEdgeSide { Top, Bottom, Left, Right }

public readonly record struct BackdropBlurOptions
{
    public BackdropBlurOptions() { }
    public float Radius { get; init; } = 8;
    public uint TintColor { get; init; } = 0xffffff;
    public float Tint { get; init; }
    internal void Validate()
    {
        if (!float.IsFinite(Radius) || Radius < 0 || Radius > 64) throw new ArgumentOutOfRangeException(nameof(Radius));
        if (!float.IsFinite(Tint) || Tint < 0 || Tint > 1) throw new ArgumentOutOfRangeException(nameof(Tint));
        if (TintColor > 0xffffff) throw new ArgumentOutOfRangeException(nameof(TintColor));
    }
}
public static class BackdropBlurEffect
{
    public static WgslProgram Program { get; } = WgslProgram.Embedded(typeof(Ui).Assembly, "Plico.backdrop_blur.wgsl", WgslMode.Backdrop);
}
public sealed unsafe partial class Ui
{
    /// Blur the application's already-painted scene. Position/size the returned
    /// surface over content; sharp controls should be declared after it.
    public Builder BackdropBlur(BackdropBlurOptions? options = null, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) =>
        BackdropSurface(options ?? new(), 0, key, file, line);

    /// A soft, progressively frosted edge, inspired by MyGO's scroll-edge material.
    /// It is a noninteractive overlay, independent from the scroll container.
    /// The selected edge is strongest; the opposite edge preserves the scene.
    public Builder ScrollEdge(ScrollEdgeSide side = ScrollEdgeSide.Top, BackdropBlurOptions? options = null, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!Enum.IsDefined(side)) throw new ArgumentOutOfRangeException(nameof(side));
        return BackdropSurface(options ?? new(), (int)side + 1, key, file, line);
    }
    private Builder BackdropSurface(BackdropBlurOptions options, int mode, string? key, string file, int line)
    {
        options.Validate(); uint color = options.TintColor;
        return Shader(BackdropBlurEffect.Program, key: key, file: file, line: line)
            .Uniform(0, new Vector4(options.Radius, mode, options.Tint, 0))
            .Uniform(1, new Vector3((color >> 16) / 255f, ((color >> 8) & 255) / 255f, (color & 255) / 255f));
    }
}
