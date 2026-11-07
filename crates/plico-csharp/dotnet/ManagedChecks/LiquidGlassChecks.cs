using Plico;
using Plico.Testing;
internal static class LiquidGlassChecks
{
    internal static void Run()
    {
        var options = new LiquidGlassOptions();
        using var h = new UiHarness(ui => {
            using (ui.LiquidGlass(options)) { ui.Text("Sharp content"); ui.Button("Glass action"); }
        });
        h.Render();
        if (!h.Nodes.Any(n => n.Text == "Sharp content")) throw new InvalidOperationException("Glass lost children");
        h.Click("Glass action"); h.Render();
        options = options with { Enabled = false }; h.Render();
        if (!h.Nodes.Any(n => n.Text == "Sharp content")) throw new InvalidOperationException("Solid fallback lost children");
        options = options with { Frost = float.NaN };
        bool rejected = false;
        try { h.Render(); } catch (ArgumentOutOfRangeException) { rejected = true; }
        if (!rejected) throw new InvalidOperationException("Invalid glass parameters accepted");
        if (LiquidGlassEffect.Program.Mode != WgslMode.Backdrop) throw new InvalidOperationException("Glass lacks backdrop input");
    }
}
