using Plico;

internal static class SdkChecks
{
    private sealed record Service(string Name);
    internal static void Run()
    {
        string draft = "initial"; int submits = 0, iconClicks = 0, selection = 0;
        var root = Theme.Light with { Accent = 0xb45309, FontSize = 16 };
        var nested = Theme.Dark with { Accent = 0x16a34a };
        using var app = new Application(ui =>
        {
            ui.Theme = root;
            if (ui.DefaultFontSize != 16) throw new InvalidOperationException("Theme font default lost");
            using (ui.Provide(new Service("outer")))
            {
                using (ui.UseTheme(nested))
                using (ui.Provide(new Service("inner")))
                    if (ui.Theme != nested || ui.Context<Service>()?.Name != "inner") throw new InvalidOperationException("Nested context lost");
                if (ui.Theme != root || ui.Context<Service>()?.Name != "outer") throw new InvalidOperationException("Context did not restore");
            }
            if (ui.Context<Service>() is not null) throw new InvalidOperationException("Context leaked outside scope");
            using (ui.Column().FillWidth().PaddingX(20).PaddingY(8).MaxWidth(600).Gap(12))
            {
                if (ui.TextInput("SDK draft", ref draft).Placeholder("Type here").MaxWidth(320).PaddingX(12).Autofocus().Submitted()) submits++;
                using (var b = ui.ButtonContent("Composed action", primary: true))
                {
                    ui.Text("★"); ui.Text("Save");
                    if (b.Clicked()) iconClicks++;
                }
                ui.Radio("First option", 0, ref selection);
                ui.Radio("Second option", 1, ref selection);
                ui.Progress(.42);
            }
        }, virtualTime: true);
        app.Step();
        app.SetText("SDK draft", "中文 draft"); app.Step();
        if (draft != "中文 draft") throw new InvalidOperationException("Style metadata corrupted input value");
        app.KeyPress("Enter"); app.Step();
        if (submits != 1) throw new InvalidOperationException($"Submission was lost or duplicated: {submits}");
        app.Click("button", "Composed action"); app.Step();
        app.Click("button", "Second option"); app.Step();
        if (iconClicks != 1 || selection != 1) throw new InvalidOperationException("Managed composition event failed");
        try { app.Run(new WindowOptions { Width = 0 }); throw new InvalidOperationException("Invalid window options accepted"); }
        catch (ArgumentOutOfRangeException) { }
        app.Step(); // Rejected configuration must not consume the host.
        Console.WriteLine("SDK theme/context, composed controls, text submission and window validation passed");
    }
}
