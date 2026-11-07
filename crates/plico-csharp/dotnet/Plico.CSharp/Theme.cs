namespace Plico;

/// Managed widget palette. Native rendering consumes resolved colors, not widget policy.
public sealed record Theme
{
    public bool IsDark { get; init; }
    public uint Surface { get; init; } = 0xf7f7f8;
    public uint Panel { get; init; } = 0xffffff;
    public uint Foreground { get; init; } = 0x202024;
    public uint Muted { get; init; } = 0x71717a;
    public uint Border { get; init; } = 0xd9d9de;
    public uint Control { get; init; } = 0xf4f4f5;
    public uint Hover { get; init; } = 0xe4e4e7;
    public uint Pressed { get; init; } = 0xd4d4d8;
    public uint Accent { get; init; } = 0x2563eb;
    public uint AccentHover { get; init; } = 0x1d4ed8;
    public uint AccentPressed { get; init; } = 0x1e40af;
    public uint OnAccent { get; init; } = 0xffffff;
    public uint Selection { get; init; } = 0xdde9ff;
    public uint FontSize { get; init; } = 14;
    public static Theme Light { get; } = new();
    public static Theme Dark { get; } = new() { IsDark = true, Surface = 0x18181b, Panel = 0x202023,
        Foreground = 0xf4f4f5, Muted = 0xa1a1aa, Border = 0x3f3f46, Control = 0x27272a,
        Hover = 0x3f3f46, Pressed = 0x52525b, Selection = 0x263d65 };
}

public sealed unsafe partial class Ui
{
    private Theme? rootTheme;
    private readonly List<(Type Type, object Value, int Token)> contexts = new();
    private int contextToken;
    public Theme Theme { get => Context<Theme>() ?? rootTheme ?? (DarkTheme ? Plico.Theme.Dark : Plico.Theme.Light); set => rootTheme = value ?? throw new ArgumentNullException(nameof(value)); }
    /// Scoped managed context; ownership is the current synchronous declaration.
    public ContextScope Provide<T>(T value) where T : class
    {
        Verify(generation);
        ArgumentNullException.ThrowIfNull(value);
        int token = checked(++contextToken);
        contexts.Add((typeof(T), value, token));
        return new(this, generation, token);
    }
    public T? Context<T>() where T : class
    {
        Verify(generation);
        for (int i = contexts.Count - 1; i >= 0; i--) if (contexts[i].Type == typeof(T)) return (T)contexts[i].Value;
        return null;
    }
    public ContextScope UseTheme(Theme theme) => Provide(theme);
    public readonly struct ContextScope : IDisposable
    {
        private readonly Ui ui; private readonly int generation, token;
        internal ContextScope(Ui ui, int generation, int token) { this.ui = ui; this.generation = generation; this.token = token; }
        public void Dispose()
        {
            ui.Verify(generation);
            if (ui.contexts.Count == 0 || ui.contexts[^1].Token != token) throw new InvalidOperationException("Context scopes must close in nesting order");
            ui.contexts.RemoveAt(ui.contexts.Count - 1);
        }
    }
}
