using System.Runtime.CompilerServices;
using System.Text;
using Plico.Interop;

namespace Plico;

/// Builds flat native batches; never calls a P/Invoke per widget. Arrays live
/// on the pinned object heap and remain rooted through the native copy.
public sealed unsafe partial class Ui
{
    private const int MaxCommands = 100_000, MaxUtf8Bytes = 16 * 1024 * 1024;
    private Command[] commands = GC.AllocateUninitializedArray<Command>(128, pinned: true);
    private byte[] bytes = GC.AllocateUninitializedArray<byte>(4096, pinned: true);
    private int commandCount, byteCount;
    private readonly Stack<int> parents = new();
    private readonly Dictionary<ulong, int> occurrences = new();
    private readonly Dictionary<ulong, InputEvent> events = new();
    private readonly List<ulong> staleStateIds = new();
    public bool DarkTheme { get; set; }
    /// Default type size for the managed composition scope; explicit sizes override it.
    private uint? defaultFontSize;
    public uint DefaultFontSize { get => defaultFontSize ?? Theme.FontSize; set => defaultFontSize = value; }
    public float Width { get; private set; }
    public float Height { get; private set; }
    public double Seconds { get; private set; }
    public int NodeCount => commandCount;
    public int ByteCount => byteCount;
    private static readonly UTF8Encoding eventEncoding = new(false, true);
    private readonly record struct InputEvent(uint Kind, double Value, string Text);

    internal void Begin(Frame* frame, bool acceptEvents)
    {
        if (frame is null || frame->version != 5 || !float.IsFinite(frame->width) || !float.IsFinite(frame->height)
            || frame->width <= 0 || frame->height <= 0 || frame->width > 16384 || frame->height > 16384
            || !double.IsFinite(frame->seconds) || frame->seconds < 0
            || frame->event_count > 100_000 || frame->utf8_len > 16 * 1024 * 1024
            || (frame->event_count != 0 && frame->events is null) || (frame->utf8_len != 0 && frame->utf8 is null))
            throw new InvalidOperationException("Invalid native frame header");
        System.Security.Cryptography.CryptographicOperations.ZeroMemory(bytes.AsSpan(0, byteCount));
        ownerThread = Environment.CurrentManagedThreadId;
        generation = checked(generation + 1); building = true;
        BeginLocalStatePass();
        properties.Clear(); contexts.Clear(); animate = false;
        commandCount = byteCount = 0; liveLists.Clear(); liveSelections.Clear(); popupLinks.Clear(); parents.Clear(); occurrences.Clear(); events.Clear();
        scopes.Clear(); seen.Clear(); seenScopes.Clear(); currentScope = 0; canvasDocuments.Clear();
        Width = frame->width; Height = frame->height; Seconds = frame->seconds;
        if (!acceptEvents) return;
        for (int i = 0; i < frame->event_count; i++)
        {
            var e = frame->events[i];
            if (e.id == 0 || e.kind is not (1 or 3 or 4 or 5 or 7 or 8) || !double.IsFinite(e.value)
                || (ulong)e.text_start + e.text_len > frame->utf8_len)
                throw new InvalidOperationException("Invalid native event ID, kind or UTF-8 span");
            string text;
            try { text = eventEncoding.GetString(new ReadOnlySpan<byte>(frame->utf8 + e.text_start, checked((int)e.text_len))); }
            catch (DecoderFallbackException error) { throw new InvalidOperationException("Invalid native event UTF-8", error); }
            if (!events.TryAdd(e.id, new(e.kind, e.value, text)))
                throw new InvalidOperationException("Duplicate native event ID");
        }
    }
    internal Batch Finish()
    {
        Verify(generation);
        if (parents.Count != 0 || scopes.Count != 0 || contexts.Count != 0) throw new InvalidOperationException("Unclosed layout or identity scope");
        // Resolve inheritance after builder chains have finished, so setting a
        // parent's font after its label was added still updates that label.
        for (int i = 0; i < commandCount; i++)
        {
            ref var command = ref commands[i];
            int parent = unchecked((int)command.parent);
            if (command.font_size == 0) command.font_size = parent < 0 ? DefaultFontSize : commands[parent].font_size;
            if (command.weight == 0) command.weight = parent < 0 ? 400U : commands[parent].weight;
            if ((command.flags & 8U) == 0)
            {
                command.foreground = parent < 0 ? Theme.Foreground : commands[parent].foreground;
                // Paragraphs have fully resolved paint styles; SVG preserves its
                // original palette unless the caller explicitly supplies a tint.
                if (command.kind == 3) command.flags |= 8U;
            }
        }
        foreach (var pair in properties)
        {
            var encoded = Encode(System.Text.Json.JsonSerializer.Serialize(pair.Value, PropertyJsonContext.Default.NodeProperties));
            commands[pair.Key].properties_start = encoded.Item1; commands[pair.Key].properties_len = encoded.Item2;
        }
        foreach (var link in popupLinks)
        {
            if (commands[link.Trigger].width >= 0) commands[link.Panel].width = commands[link.Trigger].width;
            commands[link.Portal].flags |= commands[link.Trigger].flags & 64U;
        }
        Prune(selections, liveSelections);
        Prune(listRanges, liveLists);
        CommitLocalStates();
        fixed (Command* c = commands)
        fixed (byte* b = bytes)
        {
            building = false;
            return new Batch { animate = animate ? 1U : 0U, version = 5, dark = (rootTheme?.IsDark ?? DarkTheme) ? 1U : 0U, commands = c, command_count = (uint)commandCount, utf8 = b, utf8_len = (uint)byteCount };
        }
    }
    private void Prune<T>(Dictionary<ulong, T> state, HashSet<ulong> live)
    {
        if (state.Count == 0) return;
        staleStateIds.Clear();
        foreach (ulong id in state.Keys) if (!live.Contains(id)) staleStateIds.Add(id);
        foreach (ulong id in staleStateIds) state.Remove(id);
        staleStateIds.Clear();
    }
    private static ulong Hash(string value, ulong seed)
    {
        foreach (char c in value) seed = (seed ^ c) * 1099511628211;
        return seed;
    }
    private int Add(uint kind, string text, string file, int line, string? key)
    {
        Verify(generation);
        if (commandCount >= MaxCommands) throw new InvalidOperationException("UI batch exceeds 100000 nodes");
        var parent = parents.Count == 0 ? -1 : parents.Peek();
        ulong hash = Address(kind, file, line, key);
        if (!seen.Add(hash)) throw new InvalidOperationException("Duplicate widget identity; use a unique business key or Scope");
        if (commandCount == commands.Length)
        {
            var next = GC.AllocateUninitializedArray<Command>(Math.Min(MaxCommands, commands.Length * 2), pinned: true);
            commands.AsSpan(0, commandCount).CopyTo(next); commands = next;
        }
        int index = commandCount++;
        commands[index] = new Command { id = hash == 0 ? 1 : hash, parent = unchecked((uint)parent), kind = kind, width = -1, height = -1, padding = -1, radius = -1, shrink = 1, opacity = 1 };
        (commands[index].text_start, commands[index].text_len) = Encode(text);
        return index;
    }
    private (uint, uint) Encode(string text)
    {
        int count = Encoding.UTF8.GetByteCount(text);
        if (count > MaxUtf8Bytes - byteCount) throw new InvalidOperationException("UI batch exceeds 16 MiB UTF-8 budget");
        if (byteCount + count > bytes.Length)
        {
            int capacity = Math.Min(MaxUtf8Bytes, Math.Max(bytes.Length * 2, byteCount + count));
            var next = GC.AllocateUninitializedArray<byte>(capacity, pinned: true);
            bytes.AsSpan(0, byteCount).CopyTo(next); bytes = next;
        }
        int start = byteCount;
        byteCount += Encoding.UTF8.GetBytes(text, bytes.AsSpan(byteCount));
        return ((uint)start, (uint)count);
    }
    private Builder Leaf(uint kind, string text, string file, int line, string? key) => new(this, Add(kind, text, file, line, key));
    /// Primitive composable box for managed widgets.
    public Container Box(string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) => new(this, Add(16, "", file, line, key));
    public Container Column(string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) => new(this, Add(1, "", file, line, key));
    public Container Row(string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) => new(this, Add(2, "", file, line, key));
    public Builder Text(string text, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) => Leaf(3, text, file, line, key);
    public Builder TextInput(string label, ref string value, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var b = Leaf(7, label, file, line, key).Width(240).Height(36).Padding(8).Radius(6)
            .Background(Theme.Panel).Color(Theme.Foreground)
            .Border(1, Theme.Border);
        if (events.TryGetValue(b.Id, out var e) && e.Kind == 4) value = e.Text;
        var span = Encode(value);
        commands[b.Index].value_start = span.Item1; commands[b.Index].value_len = span.Item2; return b;
    }
    public Builder Image(ImageResource resource, float width, float height, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (resource.CanvasOnly) throw new InvalidOperationException("A canvas-only raster requires ImageCanvas");
        return Image(resource.Id, width, height, key, file, line);
    }
    public Builder Image(ulong resource, float width, float height, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var b = Leaf(8, "", file, line, key); commands[b.Index].resource = resource; return b.Width(width).Height(height).Shrink();
    }
    /// WGSL effect body defining plico_effect(uv); animation stays entirely native.
    public Builder Shader(string wgsl, bool animated = false, string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var builder = Leaf(10, wgsl, file, line, key);
        if (animated) commands[builder.Index].flags |= 1U << 14;
        return builder;
    }
    public readonly partial struct Builder
    {
        private readonly Ui ui;
        private readonly int index, generation;
        internal Builder(Ui ui, int index) { this.ui = ui; this.index = index; generation = ui.generation; }
        private ref Command Current { get { ui.Verify(generation); return ref ui.commands[index]; } }
        public bool IsValid => ui is not null && ui.Valid(generation);
        internal Ui Owner => ui;
        internal int Index => index;
        public ulong Id => Current.id;
        public Builder Width(float value) { Current.flags &= ~((1U << 16) | 1U); Current.width = value; return this; }
        public Builder WidthPercent(float value) { Current.flags = (Current.flags & ~1U) | (1U << 16); Current.width = value; return this; }
        public Builder Height(float value) { Current.height = value; return this; }
        public Builder FillWidth() { Current.flags |= 1; return this; }
        public Builder FillHeight() { Current.flags |= 2; return this; }
        public Builder AriaLabel(string label) { ui.Verify(generation); var span = ui.Encode(label); Current.text_start = span.Item1; Current.text_len = span.Item2; return this; }
        public Builder Border(float width, uint color) { Current.border_width = width; Current.border_color = color; return this; }
        public Builder MinWidth(float value) { Current.min_width = value; return this; }
        public Builder MinHeight(float value) { Current.min_height = value; return this; }
        public Builder Shrink(float value = 0) { Current.shrink = value; return this; }
        public Builder Motion(bool value = true, ushort milliseconds = 180) { ui.Properties(index).TransitionMs = value ? milliseconds : (ushort)0; if (value) Current.flags |= 1U << 15; else Current.flags &= ~(1U << 15); return this; }
        public Builder Grow(float value = 1) { Current.grow = value; return this; }
        public Builder Padding(float value) { Current.padding = value; return this; }
        public Builder Radius(float value) { Current.radius = value; return this; }
        public Builder HoverBackground(uint rgb) { Current.flags |= 1U << 22; Current.hover_background = rgb; return this; }
        public Builder PressedBackground(uint rgb) { Current.flags |= 1U << 23; Current.pressed_background = rgb; return this; }
        public Builder Background(uint rgb) { Current.flags |= 4; Current.background = rgb; return this; }
        public Builder Color(uint rgb) { Current.flags |= 8; Current.foreground = rgb; return this; }
        public Builder FontSize(uint value) { Current.font_size = value; return this; }
        public Builder Weight(uint value) { Current.weight = value; return this; }
        public Builder JustifyStart() { Current.flags |= 1U << 17; return this; }
        public Builder Primary()
        {
            Current.flags |= 4U | 8U | (1U << 22) | (1U << 23);
            Current.background = ui.Theme.Accent; Current.foreground = ui.Theme.OnAccent;
            Current.hover_background = ui.Theme.AccentHover; Current.pressed_background = ui.Theme.AccentPressed;
            Current.border_width = 0; return this;
        }
        public Builder Ghost()
        {
            Current.flags &= ~4U; Current.flags |= 8U | (1U << 22) | (1U << 23);
            Current.foreground = ui.Theme.Foreground;
            Current.hover_background = ui.Theme.Control;
            Current.pressed_background = ui.Theme.Border;
            Current.border_width = 0; return this;
        }
        public Builder Disabled(bool value = true) { if (value) Current.flags |= 64; else Current.flags &= ~64U; Current.opacity = value ? 0.45f : 1; return this; }
        public IReadOnlyList<CanvasEdit> CanvasEvents()
        {
            if (Current.kind != 13) throw new InvalidOperationException("CanvasEvents requires an image canvas");
            return ui.ReadCanvas(Id);
        }
        public bool Clicked() => ui.events.TryGetValue(Id, out var e) && e.Kind == 1;
    }
    /// Both using-scopes and Children(Action) close the scope in finally/Dispose.
    public readonly partial struct Container : IDisposable
    {
        private readonly Ui ui; private readonly int index, generation, scopeDepth;
        private ref Command Current { get { ui.Verify(generation); return ref ui.commands[index]; } }
        internal int Index => index;
        public ulong Id => Current.id;
        internal Container(Ui ui, int index) { this.ui = ui; this.index = index; generation = ui.generation; scopeDepth = ui.scopes.Count; ui.parents.Push(index); }
        public Container Interactive(string accessibleName)
        {
            Current.flags |= 1U << 21;
            var span = ui.Encode(accessibleName); Current.text_start = span.Item1; Current.text_len = span.Item2;
            return this;
        }
        public Container HoverBackground(uint rgb) { Current.flags |= 1U << 22; Current.hover_background = rgb; return this; }
        public Container PressedBackground(uint rgb) { Current.flags |= 1U << 23; Current.pressed_background = rgb; return this; }
        public bool Clicked() => ui.events.TryGetValue(Current.id, out var e) && e.Kind == 1;
        public Container Color(uint rgb) { Current.flags |= 8; Current.foreground = rgb; return this; }
        public Container FontSize(uint size) { Current.font_size = size; return this; }
        public Container Weight(uint weight) { Current.weight = weight; return this; }
        public Container Width(float value) { Current.flags &= ~((1U << 16) | 1U); Current.width = value; return this; }
        public Container WidthPercent(float value) { Current.flags = (Current.flags & ~1U) | (1U << 16); Current.width = value; return this; }
        public Container Height(float value) { Current.height = value; return this; }
        public Container FillWidth() { Current.flags |= 1; return this; }
        public Container FillHeight() { Current.flags |= 2; return this; }
        public Container Border(float width, uint color) { Current.border_width = width; Current.border_color = color; return this; }
        public Container MinWidth(float value) { Current.min_width = value; return this; }
        public Container MinHeight(float value) { Current.min_height = value; return this; }
        public Container Shrink(float value = 0) { Current.shrink = value; return this; }
        public Container Motion(bool value = true, ushort milliseconds = 180) { ui.Properties(index).TransitionMs = value ? milliseconds : (ushort)0; if (value) Current.flags |= 1U << 15; else Current.flags &= ~(1U << 15); return this; }
        public Container Grow(float value = 1) { Current.grow = value; return this; }
        public Container Padding(float value) { Current.padding = value; return this; }
        public Container AlignStart() { Current.flags = (Current.flags & ~(3U << 9)) | 256; return this; }
        public Container AlignCenter() { Current.flags = (Current.flags & ~((3U << 9) | 256U)) | (1U << 9); return this; }
        public Container AlignEnd() { Current.flags = (Current.flags & ~((3U << 9) | 256U)) | (2U << 9); return this; }
        public Container AlignStretch() { Current.flags = (Current.flags & ~((3U << 9) | 256U)) | (3U << 9); return this; }
        public Container JustifyCenter() { Current.flags = (Current.flags & ~(3U << 11)) | (1U << 11); return this; }
        public Container JustifyEnd() { Current.flags = (Current.flags & ~(3U << 11)) | (2U << 11); return this; }
        public Container SpaceBetween() { Current.flags = (Current.flags & ~(3U << 11)) | (3U << 11); return this; }
        public Container Wrap(bool value = true) { if (value) Current.flags |= 1U << 13; else Current.flags &= ~(1U << 13); return this; }
        public Container Gap(float value) { Current.gap = value; return this; }
        public Container Radius(float value) { Current.radius = value; return this; }
        public Container Background(uint rgb) { Current.flags |= 4; Current.background = rgb; return this; }
        public Container ScrollY(bool enabled = true) { if (enabled) Current.flags |= 128; else Current.flags &= ~128U; return this; }
        public Container Disabled(bool value = true) { if (value) Current.flags |= 64; else Current.flags &= ~64U; Current.opacity = value ? 0.45f : 1; return this; }
        public void Children(Action content) { VerifyOpen(); try { content(); } finally { Dispose(); } }
        private void VerifyOpen()
        {
            ui.Verify(generation);
            if (ui.scopes.Count != scopeDepth || ui.parents.Count == 0 || ui.parents.Peek() != index)
                throw new InvalidOperationException("Layout scopes must close in nesting order");
        }
        public void Dispose() { VerifyOpen(); ui.parents.Pop(); }
    }
}
