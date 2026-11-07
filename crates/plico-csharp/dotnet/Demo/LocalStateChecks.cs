using Plico;

internal static class LocalStateChecks
{
    private static void Check(bool condition, string message)
    { if (!condition) throw new InvalidOperationException("Local state: " + message); }

    internal static void Run()
    {
        long[] rows = [10, 20];
        var initializations = new Dictionary<long, int>();
        var displayed = new Dictionary<long, int>();
        void Counter(Ui ui, long id)
        {
            ref int value = ref ui.State(() => { initializations[id] = initializations.GetValueOrDefault(id) + 1; return 0; });
            if (ui.Button($"Increment {id}").Clicked()) value++;
            displayed[id] = value;
        }
        using (var app = new Application(ui =>
        {
            using (ui.Column())
                foreach (long id in rows)
                    using (ui.Scope(id)) Counter(ui, id);
        }))
        {
            app.Step(); app.Step();
            Check(initializations[10] == 1 && initializations[20] == 1, "initializer repeated");
            app.Click("button", "Increment 10"); app.Step();
            Check(displayed[10] == 1 && displayed[20] == 0, "component states shared or event repeated");
            rows = [20, 10]; app.Invalidate(); app.Step();
            Check(displayed[10] == 1 && initializations[10] == 1, "keyed reordering lost state");
            rows = [20]; app.Invalidate(); app.Step();
            rows = [10, 20]; app.Invalidate(); app.Step();
            Check(displayed[10] == 0 && initializations[10] == 2 && initializations[20] == 1, "unmounted state did not reset independently");
        }

        bool alternate = false, fail = false;
        int created = 0, snapshot = 0;
        string key = "stable";
        using (var app = new Application(ui =>
        {
            ref int count = ref ui.State(() => { created++; return 0; }, key: key);
            if (ui.Button("Switch branch").Clicked()) { count++; alternate = true; }
            if (alternate) { ref int branch = ref ui.State(static () => 99, key: "branch"); _ = branch; }
            if (fail) throw new InvalidOperationException("intentional rejected local-state build");
            snapshot = count;
        }))
        {
            app.Step(); app.Click("button", "Switch branch"); app.Step();
            Check(snapshot == 1 && created == 1, "event settling reinitialized state");
            fail = true; app.Invalidate();
            bool rejected = false; try { app.Step(); } catch (InvalidOperationException) { rejected = true; }
            Check(rejected, "test build was not rejected");
            fail = false; app.Invalidate(); app.Step();
            Check(snapshot == 1 && created == 1, "failed build removed mounted state");
            key = "moved"; app.Invalidate(); app.Step();
            Check(snapshot == 0 && created == 2, "changed key did not reset local state");
        }

        int type = 0, number = 0, newCells = 0;
        bool failType = false;
        using (var app = new Application(ui =>
        {
            if (type == 0)
            {
                ref int value = ref ui.State(static () => 7, key: "typed");
                if (ui.Button("Change integer").Clicked()) value++;
                number = value;
            }
            else
            {
                ref string value = ref ui.State(static () => "replacement", key: "typed");
                Check(value == "replacement", "incompatible slot was not reset");
                ref int fresh = ref ui.State(() => ++newCells, key: "new"); _ = fresh;
                if (failType) throw new InvalidOperationException("intentional type replacement failure");
            }
        }))
        {
            app.Step(); app.Click("button", "Change integer"); app.Step(); Check(number == 8, "integer update failed");
            type = 1; failType = true; app.Invalidate(); try { app.Step(); } catch (InvalidOperationException) { }
            type = 0; failType = false; app.Invalidate(); app.Step(); Check(number == 8, "failed type migration replaced old state");
            type = 1; app.Invalidate(); app.Step(); Check(newCells == 2, "new slot from failed build survived");
            type = 0; app.Invalidate(); app.Step(); Check(number == 7, "successful incompatible type migration did not reset");
        }
        int seed = 5, calls = 0, stable = 0;
        bool branch = false, grew = false;
        Ui? retained = null;
        using (var app = new Application(ui =>
        {
            retained = ui;
            if (branch) { ref int value = ref ui.State(() => { calls++; return seed; }, key: "moved-callsite"); stable = value; }
            else { ref int value = ref ui.State(() => { calls++; return seed; }, key: "moved-callsite"); stable = value; }
            ref int first = ref ui.State(static () => 0, key: "grow-anchor");
            for (int i = 0; i < 1000; i++) { ref int item = ref ui.State(static () => 0, key: $"row-{i}"); _ = item; }
            if (grew) Check(first == 42, "ref mutation after dictionary growth missed retained storage");
            GC.Collect(); first = 42; grew = true;
            ref int duplicate = ref ui.State(static () => 0, key: "duplicate"); _ = duplicate;
            bool rejected = false;
            try { _ = ui.State(static () => 0, key: "duplicate"); } catch (InvalidOperationException) { rejected = true; }
            Check(rejected, "duplicate state key was accepted");
            Check(Task.Run(() =>
            {
                try { _ = ui.State(static () => 0, key: "worker"); return false; }
                catch (InvalidOperationException) { return true; }
            }).GetAwaiter().GetResult(), "worker thread accessed local state");
        }))
        {
            app.Step(); seed = 99; branch = true;
            HotReload.UpdateApplication(null); app.Step();
            Check(stable == 5 && calls == 1, "explicit key or metadata-update rebuild reset state");
        }
        bool outsideRejected = false;
        try { _ = retained!.State(static () => 0); } catch (InvalidOperationException) { outsideRejected = true; }
        Check(outsideRejected, "state access outside build was accepted");

        int left = 0, right = 0;
        void AutomaticCounter(Ui ui, string name, ref int snapshot)
        {
            ref int value = ref ui.State(static () => 0);
            if (ui.Button(name).Clicked()) value++;
            snapshot = value;
        }
        using (var app = new Application(ui =>
        {
            using (ui.Scope()) AutomaticCounter(ui, "Left local", ref left);
            using (ui.Scope()) AutomaticCounter(ui, "Right local", ref right);
        }))
        {
            app.Step(); app.Click("button", "Left local"); app.Step();
            Check(left == 1 && right == 0, "automatic component scopes shared local state");
        }
        Console.WriteLine("Local state checks passed: scopes, reorder, unmount, settling, failed builds, type resets, metadata updates and ref stability");
    }
}
