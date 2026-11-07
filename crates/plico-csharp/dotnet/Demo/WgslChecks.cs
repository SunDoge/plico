using Plico;

internal static class WgslChecks
{
    internal static void Run()
    {
        static string Source(string color) => $"fn plico_effect(uv: vec2<f32>) -> vec4<f32> {{ return vec4<f32>({color}, 1.0); }}";
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        Check(new WgslProgram(Source("1.0, 0.0, 0.0")).Validate() is null, "Valid WGSL rejected");
        Check(new WgslProgram("not valid WGSL").Validate() is not null, "Invalid WGSL accepted");
        string path = Path.Combine(Path.GetTempPath(), $"plico-live-{Guid.NewGuid():N}.wgsl");
        WgslFile? file = null;
        try
        {
            File.WriteAllText(path, Source("1.0, 0.0, 0.0"));
            using var app = new Application(ui => { if (file?.Current is { } program) ui.Shader(program, animated: true).Width(128).Height(128); }, virtualTime: true);
            using var asset = file = new WgslFile(path, app, watch: true);
            Check(asset.Error is null && asset.Revision == 1, "Initial shader validation failed");
            void Wait(Func<bool> done)
            {
                var timer = System.Diagnostics.Stopwatch.StartNew();
                while (!done()) { app.Step(); if (timer.Elapsed.TotalSeconds > 8) throw new TimeoutException($"WGSL watch timed out: {asset.Error}"); Thread.Sleep(5); }
                app.Step();
            }
            void Capture(string name)
            {
                if (Environment.GetEnvironmentVariable("PLICO_WGSL_CAPTURE_DIR") is { } directory)
                { Directory.CreateDirectory(directory); app.Capture(Path.Combine(directory, name + ".png"), 128, 128); }
            }
            app.Step(); Capture("red");
            ulong callbacks = app.Stats.Callbacks;
            for (ulong time = 16; time <= 80; time += 16) { app.SetTime(time); app.Step(); }
            Check(app.Stats.Callbacks == callbacks, "Shader animation re-entered C# declaration");
            // Real filesystem notifications, with no explicit ReloadAsync or compiler.
            File.WriteAllText(path, Source("0.0, 1.0, 0.0")); Wait(() => asset.Revision == 2); Capture("green");
            File.WriteAllText(path, "fn broken("); Wait(() => asset.Error is not null); Capture("invalid-keeps-green");
            Check(asset.Current?.Source == Source("0.0, 1.0, 0.0") && asset.Revision == 2, "Invalid shader replaced the visible program");
            File.WriteAllText(path, Source("0.0, 0.0, 1.0")); Wait(() => asset.Revision == 3 && asset.Error is null); Capture("blue");
            Console.WriteLine($"WGSL live file reload and last-good recovery passed in PID {Environment.ProcessId}");
        }
        finally { file?.Dispose(); File.Delete(path); }
    }
}
