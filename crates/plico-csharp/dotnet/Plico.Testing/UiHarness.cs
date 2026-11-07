using System.Text;
using Plico.Interop;
namespace Plico.Testing;

public enum TestRole { Container, Text, Button, CheckBox, Switch, TextBox, Slider, ComboBox, Option, ListBox, Image, Shader, Portal, Dialog, VirtualList, Other }

/// An owned managed declaration snapshot. It contains no pointers or live builders.
public sealed record TestNode(ulong Id, int Parent, TestRole Role, string Text, string Value, string Properties,
    bool Disabled, bool Clickable, double Number);

/// Executes the production managed builder and settling logic without loading the
/// Rust library, a font engine, a window or GPU. Does not simulate layout, native
/// text editing, hit testing or focus: use native integration checks for those.
public sealed unsafe class UiHarness : IDisposable
{
    private readonly Ui ui = new();
    private readonly Action<Ui> build;
    private readonly int owner = Environment.CurrentManagedThreadId;
    private readonly List<(ulong Id, uint Kind, double Value, string Text)> pending = new();
    private IReadOnlyList<TestNode> nodes = Array.Empty<TestNode>();
    private bool disposed, rendering;
    public IReadOnlyList<TestNode> Nodes => nodes;
    public int BuildCount { get; private set; }
    public bool RequestsAnimation { get; private set; }
    public UiHarness(Action<Ui> build) => this.build = build ?? throw new ArgumentNullException(nameof(build));

    public IReadOnlyList<TestNode> Render(double seconds = 0, uint width = 1280, uint height = 900)
    {
        Verify();
        if (rendering) throw new InvalidOperationException("A harness cannot render recursively");
        var input = new Event[pending.Count];
        byte[] text = Encoding.UTF8.GetBytes(string.Concat(pending.Select(e => e.Text)));
        uint offset = 0;
        for (int i = 0; i < input.Length; i++)
        {
            uint count = checked((uint)Encoding.UTF8.GetByteCount(pending[i].Text));
            input[i] = new Event { id = pending[i].Id, kind = pending[i].Kind, value = pending[i].Value, text_start = offset, text_len = count };
            offset += count;
        }
        pending.Clear(); rendering = true;
        try
        {
            fixed (Event* events = input)
            fixed (byte* bytes = text)
            {
                var frame = new Frame { version = NativeRuntime.AbiVersion, width = width, height = height, seconds = seconds,
                    events = events, event_count = (uint)input.Length, utf8 = bytes, utf8_len = (uint)text.Length };
                var batch = ui.Declare(&frame, build);
                var result = new TestNode[batch.command_count];
                string Decode(uint start, uint length) => Encoding.UTF8.GetString(new ReadOnlySpan<byte>(batch.utf8 + start, checked((int)length)));
                for (int i = 0; i < result.Length; i++)
                {
                    var c = batch.commands[i];
                    int parent = unchecked((int)c.parent);
                    bool disabled = (c.flags & 64) != 0 || parent >= 0 && result[parent].Disabled;
                    result[i] = new(c.id, parent, Role(c), Decode(c.text_start, c.text_len), Decode(c.value_start, c.value_len),
                        Decode(c.properties_start, c.properties_len), disabled, (c.flags & (1U << 21)) != 0, c.value);
                }
                RequestsAnimation = batch.animate != 0;
                nodes = Array.AsReadOnly(result);
            }
            return nodes;
        }
        finally { BuildCount += ui.LastPassCount; rendering = false; }
    }
    public TestNode Find(string label, TestRole? role = null)
    {
        Verify();
        var matches = nodes.Where(n => n.Text == label && (role is null || n.Role == role)).ToArray();
        if (matches.Length != 1) throw new InvalidOperationException($"Expected one node '{label}', found {matches.Length}; use an ID or unique accessible label");
        return matches[0];
    }
    public void Click(string label) => Click(Unique(label, n => n.Clickable));
    public void Click(ulong id)
    {
        Verify();
        var node = nodes.Single(n => n.Id == id);
        if (!node.Clickable) throw new InvalidOperationException("Node is not an activation surface");
        Enqueue(node, 1, 0, "");
    }
    public void SetText(string label, string text) => Enqueue(Input(label, TestRole.TextBox), 4, 0, text);
    public void SetValue(string label, double value) => Enqueue(Input(label, TestRole.Slider), 3, value, "");
    private TestNode Input(string label, TestRole role)
    {
        ulong id = Unique(label, node => node.Role == role);
        return nodes.Single(node => node.Id == id);
    }
    private ulong Unique(string label, Func<TestNode, bool> predicate)
    {
        Verify();
        var matches = nodes.Where(n => n.Text == label && predicate(n)).ToArray();
        if (matches.Length != 1) throw new InvalidOperationException($"Expected one interactive node '{label}', found {matches.Length}");
        return matches[0].Id;
    }
    private void Enqueue(TestNode node, uint kind, double value, string text)
    {
        Verify();
        ArgumentNullException.ThrowIfNull(text);
        if (node.Disabled) throw new InvalidOperationException("Cannot activate a disabled node");
        if (!double.IsFinite(value)) throw new ArgumentOutOfRangeException(nameof(value));
        if (pending.Any(e => e.Id == node.Id)) throw new InvalidOperationException("Only one event per node is allowed before Render");
        if (pending.Count >= 100_000) throw new InvalidOperationException("Event batch exceeds 100000 events");
        pending.Add((node.Id, kind, value, text));
    }
    private static TestRole Role(Command c) => c.semantic_role switch
    {
        1 => TestRole.CheckBox, 2 => TestRole.Switch, 3 => TestRole.ComboBox,
        4 => TestRole.Option, 5 => TestRole.ListBox,
        _ => c.kind switch
        {
            3 => TestRole.Text, 7 => TestRole.TextBox, 19 => TestRole.Slider,
            8 => TestRole.Image, 10 => TestRole.Shader, 15 => TestRole.VirtualList,
            17 => TestRole.Portal, 18 => TestRole.Dialog,
            16 when (c.flags & (1U << 21)) != 0 => TestRole.Button,
            1 or 2 or 16 => TestRole.Container, _ => TestRole.Other
        }
    };
    private void Verify()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (Environment.CurrentManagedThreadId != owner) throw new InvalidOperationException("Harness belongs to its creating thread");
    }
    public void Dispose()
    {
        if (disposed) return;
        Verify();
        if (rendering) throw new InvalidOperationException("Cannot dispose during Render");
        disposed = true; pending.Clear(); nodes = Array.Empty<TestNode>(); ui.ClearLocalStates();
    }
}
