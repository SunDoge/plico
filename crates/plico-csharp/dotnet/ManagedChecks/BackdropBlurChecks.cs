using System.Text.Json;
using Plico;
using Plico.Testing;
internal static class BackdropBlurChecks
{
    internal static void Run()
    {
        var options = new BackdropBlurOptions { Radius = 12, Tint = .3f, TintColor = 0x123456 };
        var side = ScrollEdgeSide.Right;
        using var h = new UiHarness(ui => {
            ui.BackdropBlur(options, key: "uniform").Width(200).Height(80);
            ui.ScrollEdge(side, options, key: "edge").Width(200).Height(80);
        });
        var nodes = h.Render().Where(node => node.Role == TestRole.Shader).ToArray();
        if (nodes.Length != 2) throw new InvalidOperationException("Backdrop surfaces lost");
        for (int i = 0; i < nodes.Length; i++)
        {
            using var json = JsonDocument.Parse(nodes[i].Properties);
            var shader = json.RootElement.GetProperty("shader");
            if (!shader.GetProperty("backdrop").GetBoolean()) throw new InvalidOperationException("Backdrop binding missing");
            var values = shader.GetProperty("values");
            if (values[0].GetSingle() != 12 || values[1].GetSingle() != (i == 0 ? 0 : 4) || values[2].GetSingle() != .3f)
                throw new InvalidOperationException("Backdrop uniforms lost");
        }
        options = options with { Radius = float.NaN };
        bool rejected = false;
        try { h.Render(); } catch (ArgumentOutOfRangeException) { rejected = true; }
        if (!rejected) throw new InvalidOperationException("Invalid blur radius accepted");
        options = new(); side = (ScrollEdgeSide)100;
        rejected = false;
        try { h.Render(); } catch (ArgumentOutOfRangeException) { rejected = true; }
        if (!rejected) throw new InvalidOperationException("Invalid scroll edge accepted");
    }
}
