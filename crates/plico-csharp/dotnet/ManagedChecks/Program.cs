using Plico;
using Plico.Testing;

if (args is ["--native"])
{
    try
    {
        NativeRuntime.CheckCompatibility();
        Console.WriteLine($"Native ready: ABI {NativeRuntime.AbiVersion}, protocol {NativeRuntime.ProtocolRevision}, {NativeRuntime.LibraryPath ?? "system search"}");
    }
    catch (InvalidOperationException error) { Console.Error.WriteLine(error.Message); Environment.ExitCode = 1; }
    return;
}
if (args.Length != 0) throw new ArgumentException("Use no arguments for pure managed checks, or --native for the explicit native handshake");
void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }

// A deliberately nonexistent path proves that pure widget checks do not load Rust.
string? oldPath = Environment.GetEnvironmentVariable("PLICO_NATIVE_LIBRARY");
Environment.SetEnvironmentVariable("PLICO_NATIVE_LIBRARY", Path.Combine(Path.GetTempPath(), "plico-must-not-load", "missing-native-library"));
try
{
    WgslChecks.Run();
    LiquidGlassChecks.Run();
    ShadowChecks.Run();
    int left = 0, right = 0; bool mounted = true;
    using (var h = new UiHarness(ui => { if (mounted) { Views.Counter(ui, ref left, "Left"); Views.Counter(ui, ref right, "Right"); } }))
    {
        h.Render(); h.Click("Increment Left"); h.Render();
        Check(left == 1 && right == 0, "Intercepted local component state collided");
        Check(h.Nodes.Any(n => n.Role == TestRole.Text && n.Text == "Left: 1"), "Label before inline handler did not settle");
        mounted = false; h.Render(); mounted = true; h.Render();
        Check(left == 0 && right == 0, "Unmount did not release local state");
    }
    string email = "", query = ""; double number = 2, slider = 40; bool toggle = false;
    using (var h = new UiHarness(ui => {
        using (ui.Column()) {
            ui.TextInput("Email", ref email);
            ui.SearchField("Search", ref query);
            ui.NumberInput("Copies", ref number, 0, 10);
            ui.Slider("Volume", ref slider, 0, 100);
            ui.Checkbox("Enabled", ref toggle);
            using (ui.Column().Disabled()) ui.Button("Unavailable");
        }
    }))
    {
        h.Render(); h.SetText("Email", "user@example.com"); h.Render(); Check(email == "user@example.com", "String edit failed");
        h.SetText("Search", "test"); h.Render(); h.Click("Clear Search"); h.Render(); Check(query == "", "Search clear failed");
        h.SetText("Copies", "-"); h.Render(); Check(number == 2, "Partial numeric edit corrupted model");
        h.SetText("Copies", "5"); h.Render(); h.Click("Increase Copies"); h.Render(); Check(number == 6, "Typed number action failed");
        h.SetValue("Volume", 75); h.Render(); Check(slider == 75, "Slider value message failed");
        h.Click("Enabled"); h.Render(); Check(toggle, "Checkbox policy failed");
        bool disabled = false; try { h.Click("Unavailable"); } catch (InvalidOperationException) { disabled = true; }
        Check(disabled, "Inherited disabled state was ignored");
    }
    int page = 1; double seenTime = 0;
    using (var h = new UiHarness(ui => {
        ui.Pagination(ref page, int.MaxValue); seenTime = ui.Seconds;
        using (ui.Column().Radius(28).CornerSmoothing(1).Blur(5)) ui.Text("Filtered");
    }))
    {
        var snapshot = h.Render(seconds: 1.25); Check(snapshot.Count < 60 && seenTime == 1.25, "Clock or bounded pagination failed");
        h.Click("Next page"); h.Render(seconds: 2); Check(page == 2, "Page action failed");
        Check(snapshot.Any(n => n.Properties.Contains("corner_smoothing") && n.Properties.Contains("foreground_blur")), "Primitive properties missing");
        Check(snapshot.Any(n => n.Text == "Filtered"), "Owned snapshot did not survive another build");
    }
    using (var dispatcher = new ManualDispatcher())
    using (var request = new UiTask<string>(dispatcher))
    {
        void PumpUntil(Func<bool> done)
        {
            var clock = System.Diagnostics.Stopwatch.StartNew();
            while (!done()) { dispatcher.Pump(); if (clock.Elapsed.TotalSeconds > 3) throw new TimeoutException("Managed task did not settle"); Thread.Sleep(1); }
        }
        request.Start(_ => Task.FromResult("ready"));
        PumpUntil(() => request.Status == UiTaskStatus.Succeeded);
        Check(request.Result == "ready", "Managed async completion was lost");
        var obsolete = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);
        using var started = new ManualResetEventSlim();
        request.Start(_ => { started.Set(); return obsolete.Task; });
        Check(started.Wait(TimeSpan.FromSeconds(1)), "Previous operation did not start");
        var previous = request.Completion;
        request.Start(_ => Task.FromResult("current"));
        PumpUntil(() => request.Status == UiTaskStatus.Succeeded);
        obsolete.SetResult("obsolete"); PumpUntil(() => previous.IsCompleted);
        Check(request.Result == "current", "Obsolete async result replaced current state");
        request.Start(_ => Task.FromException<string>(new InvalidOperationException("expected")));
        PumpUntil(() => request.Status == UiTaskStatus.Failed);
        Check(request.Error?.Message == "expected", "Managed task error was lost");
        request.Start(token => Task.Delay(10000, token).ContinueWith(_ => "cancelled", token));
        request.Cancel();
        Check(request.Status == UiTaskStatus.Cancelled, "Managed task cancellation failed");
    }
    NativeRuntime.Validate(NativeRuntime.AbiVersion, 160, NativeRuntime.ProtocolRevision, 160);
    foreach (var version in new[] { (4U, 160U, 1U), (5U, 144U, 1U), (5U, 160U, 0U) })
    {
        bool mismatch = false;
        try { NativeRuntime.Validate(version.Item1, version.Item2, version.Item3, 160); }
        catch (InvalidOperationException error) { mismatch = error.Message.Contains("cargo build") && error.Message.Contains("protocol"); }
        Check(mismatch, "Mismatch did not provide an actionable diagnostic");
    }
    Console.WriteLine("Managed component checks passed; no Rust library, Cargo, window, font engine or GPU was used.");
}
finally { Environment.SetEnvironmentVariable("PLICO_NATIVE_LIBRARY", oldPath); }

internal static class Views
{
    [Component] internal static void Counter(Ui ui, ref int observed, string title)
    {
        ref int count = ref ui.State(static () => 0);
        using (ui.Column()) {
            ui.Text($"{title}: {count}");
            if (ui.Button($"Increment {title}").Clicked()) count++;
        }
        observed = count;
    }
}
