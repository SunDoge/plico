using System.Numerics;
using Plico;
using Plico.Testing;

internal static class WgslChecks
{
    internal static void Run()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        var program = new WgslProgram("test", "managed.wgsl");
        using (var harness = new UiHarness(ui => ui.Shader(program).Uniform(0, .5f).Uniform(2, new Vector2(1, 2))))
        {
            var node = harness.Render().Single();
            using var json = System.Text.Json.JsonDocument.Parse(node.Properties);
            var values = json.RootElement.GetProperty("shader").GetProperty("values");
            Check(values.GetArrayLength() == 12 && values[0].GetSingle() == .5f && values[4].GetSingle() == 0 && values[8].GetSingle() == 1 && values[9].GetSingle() == 2, "WGSL vec4 packing failed");
        }
        string path = Path.Combine(Path.GetTempPath(), $"plico-wgsl-{Guid.NewGuid():N}.wgsl");
        try
        {
            File.WriteAllText(path, "first");
            using var dispatcher = new ManualDispatcher();
            using var file = new WgslFile(path, dispatcher, validator: p => p.Source == "broken" ? "expected error" : null);
            void Wait(Task task)
            {
                var timer = System.Diagnostics.Stopwatch.StartNew();
                while (!task.IsCompleted) { dispatcher.Pump(); if (timer.Elapsed.TotalSeconds > 5) throw new TimeoutException("WGSL publication timed out"); Thread.Sleep(1); }
                task.GetAwaiter().GetResult();
            }
            Check(file.Current?.Source == "first" && file.Revision == 1, "Initial WGSL load failed");
            Wait(file.ReloadAsync()); Check(dispatcher.Invalidations == 0, "Unchanged source woke UI");
            File.WriteAllText(path, "second"); Wait(file.ReloadAsync());
            Check(file.Current?.Source == "second" && file.Revision == 2, "WGSL update failed");
            File.WriteAllText(path, "broken"); Wait(file.ReloadAsync());
            Check(file.Current?.Source == "second" && file.Error == "expected error" && file.Revision == 2, "Invalid WGSL replaced last good program");
            File.WriteAllText(path, "second"); Wait(file.ReloadAsync());
            Check(file.Error is null && file.Revision == 2, "WGSL error recovery failed");
            File.WriteAllText(path, "obsolete"); var obsolete = file.ReloadAsync();
            File.WriteAllText(path, "latest"); Wait(file.ReloadAsync()); Wait(obsolete);
            Check(file.Current?.Source == "latest", "Stale WGSL completion won");
            File.WriteAllText(path, "disposed"); var pending = file.ReloadAsync(); file.Dispose(); Wait(pending);
            Check(file.Current?.Source == "latest", "Disposed WGSL asset published");
            File.WriteAllBytes(path, [0xff]);
            bool strict = false; try { WgslProgram.Load(path); } catch (System.Text.DecoderFallbackException) { strict = true; }
            Check(strict, "Invalid UTF8 silently accepted");
        }
        finally { File.Delete(path); }
        Console.WriteLine("Managed WGSL packing, reload, error retention, cancellation checks passed");
    }
}
