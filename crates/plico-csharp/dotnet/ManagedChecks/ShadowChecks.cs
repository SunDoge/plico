using System.Text.Json;
using Plico;
using Plico.Testing;

internal static class ShadowChecks
{
    internal static void Run()
    {
        bool clear = false;
        using var ui = new UiHarness(cx => {
            using var panel = cx.Column().Shadow(y: 3, blur: 12, spread: -1, opacity: .08f);
            if (clear) panel.NoShadow();
        });
        using (var json = JsonDocument.Parse(ui.Render().Single().Properties))
        {
            var shadow = json.RootElement.GetProperty("shadow");
            if (shadow.GetProperty("offset_x").GetSingle() != 0 || shadow.GetProperty("color").GetUInt32() != 0
                || shadow.GetProperty("offset_y").GetSingle() != 3 || shadow.GetProperty("blur").GetSingle() != 12)
                throw new InvalidOperationException("Shadow zero/default values were lost in batch serialization");
        }
        clear = true;
        using (var json = JsonDocument.Parse(ui.Render().Single().Properties))
            if (json.RootElement.TryGetProperty("shadow", out _)) throw new InvalidOperationException("NoShadow did not clear the batch property");
        using var invalid = new UiHarness(cx => { using var _ = cx.Box().Shadow(blur: float.NaN); });
        bool rejected = false; try { invalid.Render(); } catch (ArgumentOutOfRangeException) { rejected = true; }
        if (!rejected) throw new InvalidOperationException("Invalid shadow reached native batch");
        Console.WriteLine("Managed shadow serialization, removal and range checks passed");
    }
}
