using System.Text.Json;
using System.Text.Json.Serialization;
namespace Plico;

public enum WindowBackground { Opaque, Transparent, Blurred, Mica, MicaAlt }
public enum WindowLevel { Normal, AlwaysOnTop, AlwaysOnBottom }
public enum WindowInputMode { Interactive, Passthrough }

/// Initial host configuration. Compositor support varies; a Wayland window is
/// not a layer-shell overlay. Transparent content is still the application's job.
public sealed record WindowOptions
{
    public string Title { get; init; } = "Plico × C#";
    public uint Width { get; init; } = 1040;
    public uint Height { get; init; } = 780;
    public uint MinWidth { get; init; } = 640;
    public uint MinHeight { get; init; } = 480;
    public uint? MaxWidth { get; init; }
    public uint? MaxHeight { get; init; }
    public bool Resizable { get; init; } = true;
    public bool Decorations { get; init; } = true;
    public bool Centered { get; init; }
    public uint CornerRadius { get; init; }
    public WindowBackground Background { get; init; }
    public WindowLevel WindowLevel { get; init; }
    public WindowInputMode InputMode { get; init; }
    public bool ActivateOnShow { get; init; } = true;
    private static readonly JsonSerializerOptions json = new() {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        Converters = { new JsonStringEnumConverter<WindowBackground>(JsonNamingPolicy.CamelCase, allowIntegerValues: false),
            new JsonStringEnumConverter<WindowLevel>(JsonNamingPolicy.CamelCase, allowIntegerValues: false),
            new JsonStringEnumConverter<WindowInputMode>(JsonNamingPolicy.CamelCase, allowIntegerValues: false) }
    };
    private static readonly WindowJsonContext serialization = new(json);
    internal byte[] Encode()
    {
        foreach (uint? size in new uint?[] { Width, Height, MinWidth, MinHeight, MaxWidth, MaxHeight })
            if (size is uint value && (value == 0 || value > 8192)) throw new ArgumentOutOfRangeException(nameof(Width), "Window dimensions must be 1..8192");
        if (MaxWidth < MinWidth || MaxHeight < MinHeight || CornerRadius > 1024 || Title is null || Title.Contains('\0') || System.Text.Encoding.UTF8.GetByteCount(Title) > 4096)
            throw new ArgumentException("Invalid window constraints or title");
        return JsonSerializer.SerializeToUtf8Bytes(this, serialization.WindowOptions);
    }
}
