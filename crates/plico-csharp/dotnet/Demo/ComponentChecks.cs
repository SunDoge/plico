using Plico;

internal static partial class ComponentChecks
{
    private static readonly Dictionary<string, int> values = new();

    [Component]
    private static void Counter(Ui ui, string name, int initial = 0)
    {
        ref int count = ref ui.State(() => initial);
        ui.Text($"{name}: {count}");
        if (ui.Button($"Increment {name}").Clicked()) count++;
        values[name] = count;
    }
    [Component]
    private static void FirstKind(Ui ui)
    {
        ref int value = ref ui.State(static () => 3, key: "value"); values["kind"] = value;
    }
    [Component]
    private static void SecondKind(Ui ui)
    {
        ref int value = ref ui.State(static () => 99, key: "value"); values["kind"] = value;
    }
    internal static void Run()
    {
        bool mounted = true;
        using var app = new Application(ui => {
            if (mounted)
            {
                Counter(ui, "First");
                Counter(ui, "Second", 10);
                using (ui.Scope(10L)) Counter(ui, "Keyed");
            }
        });
        app.Step(); app.Click("button", "Increment First"); app.Step();
        if (values["First"] != 1 || values["Second"] != 10 || values["Keyed"] != 0)
            throw new InvalidOperationException("Generated component wrappers shared local state");
        app.Click("button", "Increment Second"); app.Step();
        if (values["First"] != 1 || values["Second"] != 11) throw new InvalidOperationException("Component event repeated or reached wrong instance");
        mounted = false; app.Invalidate(); app.Step();
        mounted = true; app.Invalidate(); app.Step();
        if (values["First"] != 0 || values["Second"] != 10) throw new InvalidOperationException("Generated components failed to unmount");
        bool alternate = false;
        using var types = new Application(ui => { if (alternate) SecondKind(ui); else FirstKind(ui); });
        types.Step();
        if (values["kind"] != 3) throw new InvalidOperationException("Initial component state failed");
        alternate = true; types.Invalidate(); types.Step();
        if (values["kind"] != 99) throw new InvalidOperationException("Different component types shared state at one source line");
        Console.WriteLine("Generated component checks passed: ordinary calls, independent scopes, optional arguments, remount and component-type salts");
    }
}
