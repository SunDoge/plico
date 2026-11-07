using Plico;

internal static class IdentityChecks
{
    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
    private static void Reject(Action action, string message)
    {
        try { action(); }
        catch (InvalidOperationException) { return; }
        throw new InvalidOperationException(message);
    }
    private static ulong Item(Ui ui, long id, Dictionary<long, int> values)
    {
        var button = ui.Button($"Item {id}");
        if (button.Clicked()) values[id]++;
        return button.Id;
    }
    public static void Run()
    {
        long[] order = [10, 20];
        var ids = new Dictionary<long, ulong>();
        var values = new Dictionary<long, int> { [10] = 0, [20] = 0 };
        Ui.Builder previous = default;
        Ui? retained = null;
        bool first = true;
        using var app = new Application(ui =>
        {
            if (!first) Reject(() => previous.Color(0), "old builder was accepted in a new build");
            retained = ui;
            using (ui.Column())
            {
                ids.Clear();
                foreach (long id in order)
                {
                    using (ui.Scope(id)) ids[id] = Item(ui, id, values);
                }
                previous = ui.Text("temporary");
                var scope = ui.Scope("nesting");
                var layout = ui.Row();
                Reject(scope.Dispose, "identity scope closed through an open layout");
                layout.Dispose();
                bool enteredClosed = false;
                Reject(() => layout.Children(() => enteredClosed = true), "closed layout accepted children");
                Check(!enteredClosed, "closed layout executed a child callback");
                scope.Dispose();
                try
                {
                    using (ui.Scope("exception")) throw new ArgumentException("expected");
                }
                catch (ArgumentException) { }
                int beforeEmptyScope = ui.NodeCount;
                using (ui.Scope("empty")) { }
                Check(ui.NodeCount == beforeEmptyScope, "identity scopes must not add layout nodes");
                using (ui.Scope("duplicate")) { }
                Reject(() => ui.Scope("duplicate"), "duplicate business scope was accepted");
                ui.Text("explicit", key: "same");
                Reject(() => ui.Text("duplicate", key: "same"), "duplicate widget key was accepted");
                bool wrongThread = Task.Run(() =>
                {
                    try { previous.Color(0); return false; }
                    catch (InvalidOperationException) { return true; }
                }).GetAwaiter().GetResult();
                Check(wrongThread, "worker mutated a live UI builder");
            }
            first = false;
        });
        app.Step();
        ulong first10 = ids[10], first20 = ids[20];
        Check(first10 != first20, "business scopes produced identical widget IDs");
        Reject(() => previous.Color(0), "builder survived Finish");
        Reject(() => retained!.Text("outside"), "UI accepted declaration outside build");
        order = [20, 10]; app.Invalidate(); app.Step();
        Check(ids[10] == first10 && ids[20] == first20, "reordering changed business-scoped identity");
        app.Click("button", "Item 10"); app.Step();
        Check(values[10] == 1 && values[20] == 0, "reordered event reached wrong component");

        ulong left = 0, right = 0;
        using var automatic = new Application(ui =>
        {
            using (ui.Scope()) left = ui.Button("left").Id;
            using (ui.Scope()) right = ui.Button("right").Id;
        });
        automatic.Step();
        Check(left != right, "automatic call-site scopes collided");

        bool alternate = false;
        ulong explicitId = 0;
        using var branches = new Application(ui =>
        {
            if (alternate) explicitId = ui.Button("same", key: "stable").Id;
            else explicitId = ui.Button("same", key: "stable").Id;
        });
        branches.Step(); ulong original = explicitId;
        alternate = true; branches.Invalidate(); branches.Step();
        Check(original == explicitId, "explicit key was tied to its source branch");
        Check(!default(Ui.Builder).IsValid, "default builder reported valid");
    }
}
