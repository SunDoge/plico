using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text.Json;
using Plico;
using Plico.Interop;

internal static unsafe class Program
{
    private sealed class Fixture(int count, bool buttons, string mode, bool selects = false)
    {
        private readonly string[] a = Enumerable.Range(0, count).Select(i => $"Row {i:D5} : A").ToArray();
        private readonly string[] b = Enumerable.Range(0, count).Select(i => $"Row {i:D5} : B").ToArray();
        private readonly string[] selection = Enumerable.Repeat("a", count).ToArray();
        private static readonly SelectOption[] choices = [new("a", "Alpha"), new("b", "Beta"), new("c", "Gamma")];
        internal bool Version;
        internal void Build(Ui ui)
        {
            using (ui.Column().FillWidth().FillHeight().ScrollY())
                for (int i = 0; i < count; i++)
                    using (ui.Scope((ulong)i))
                    {
                        string label = Version && (mode == "all" || mode == "sparse" && i == count / 2) ? b[i] : a[i];
                        if (selects) ui.Select(label, ref selection[i], choices);
                        else if (buttons) ui.Button(label);
                        else ui.Text(label);
                    }
        }
    }
    private static double Us(long ticks) => ticks * 1_000_000.0 / Stopwatch.Frequency;
    private static object Distribution(double[] values)
    {
        var sorted = values.Order().ToArray();
        double P(double p) => sorted[Math.Clamp((int)Math.Ceiling(p * sorted.Length) - 1, 0, sorted.Length - 1)];
        return new { mean_us = values.Average(), p50_us = P(.5), p95_us = P(.95), p99_us = P(.99) };
    }
    private static void Main(string[] args)
    {
        BoundaryChecks.Run();
        if (args.Contains("--boundaries-only")) return;
        int samples = args.Length > 0 ? int.Parse(args[0]) : 64;
        if (samples < 16 || samples > 10000) throw new ArgumentOutOfRangeException(nameof(samples));
        string output = args.Length > 1 ? args[1] : "reports/csharp/interop.json";
        var cases = new List<object>();
        foreach (int count in new[] { 100, 1000, 10000 })
            foreach (string mode in new[] { "idle", "same", "sparse", "all" })
            {
                cases.Add(Measure(count, false, mode, samples));
                Console.Error.WriteLine($"Measured text {count} / {mode}");
            }
        foreach (int count in new[] { 100, 1000 })
        {
            cases.Add(Measure(count, true, "same", samples));
            Console.Error.WriteLine($"Measured buttons {count}");
        }
        foreach (int count in new[] { 100, 1000 }) cases.Add(Measure(count, true, "same", samples, selects: true));
        cases.Add(RoundTrip(samples));
        var report = new {
            runtime = RuntimeInformation.FrameworkDescription, os = RuntimeInformation.OSDescription,
            architecture = RuntimeInformation.ProcessArchitecture.ToString(), samples,
            command_bytes = sizeof(Command), abi = Native.plico_abi_version(),
            tiered_compilation = Environment.GetEnvironmentVariable("DOTNET_TieredCompilation"),
            note = "No GPU/window. Full nonvirtualized trees. Precomputed strings. Build-only is isolated; native phase means come from the real Step path. Step includes CPU layout/text/scene. Microsecond native counters are quantized. Managed allocations only; payload bytes are not heap residency. Idle forces a CPU Step, not a measurement of sleeping application CPU.",
            cases
        };
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(output))!);
        string json = JsonSerializer.Serialize(report, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(output, json); Console.WriteLine(json);
    }
    private static object Measure(int count, bool buttons, string mode, int samples, bool selects = false)
    {
        var fixture = new Fixture(count, buttons, mode, selects);
        var ui = new Ui();
        void BuildOnly()
        {
            fixture.Version = !fixture.Version;
            Frame frame = new() { version = 5, width = 1000, height = 800 };
            ui.Begin(&frame, acceptEvents: true);
            try { fixture.Build(ui); _ = ui.Finish(); }
            finally { ui.EndBuild(); }
        }
        for (int i = 0; i < 128; i++) BuildOnly();
        double[] buildTimes = new double[samples];
        long allocation = GC.GetAllocatedBytesForCurrentThread();
        for (int i = 0; i < samples; i++)
        { long start = Stopwatch.GetTimestamp(); BuildOnly(); buildTimes[i] = Us(Stopwatch.GetTimestamp() - start); }
        double buildAllocation = (GC.GetAllocatedBytesForCurrentThread() - allocation) / (double)samples;
        // Measure the actual owned-copy/validation boundary without layout or text.
        // A host merely initializes the SDK's native-library resolver here.
        using var resolver = new Application(_ => { });
        Batch prepared;
        Frame transportFrame = new() { version = 5, width = 1000, height = 800 };
        ui.Begin(&transportFrame, true); fixture.Build(ui); prepared = ui.Finish(); ui.EndBuild();
        void CopyOnly()
        { if (Native.plico_test_decode_batch(prepared) != 1) throw new Exception("Native preflight failed"); }
        for (int i = 0; i < 16; i++) CopyOnly();
        double[] copies = new double[samples];
        for (int i = 0; i < samples; i++)
        { long start = Stopwatch.GetTimestamp(); CopyOnly(); copies[i] = Us(Stopwatch.GetTimestamp() - start); }
        GC.KeepAlive(ui);
        if (count == 10000 && mode == "all")
            return new { count, widgets = "text", mode, nodes = ui.NodeCount, utf8_bytes = ui.ByteCount,
                payload_bytes = ui.NodeCount * sizeof(Command) + ui.ByteCount,
                isolated_managed_build = Distribution(buildTimes), build_alloc_bytes = buildAllocation,
                copy_validate = Distribution(copies),
                note = "CPU frame omitted for 10000 fully changing labels: this benchmark isolates frontend/transport rather than bulk text shaping." };
        using var app = new Application(fixture.Build, virtualTime: true);
        void Step()
        {
            fixture.Version = !fixture.Version;
            if (mode != "idle") app.Invalidate();
            app.Step(1000, 800);
        }
        app.Step(1000, 800);
        for (int i = 0; i < 16; i++) Step();
        BridgeStats before = app.Stats;
        double[] total = new double[samples];
        allocation = GC.GetAllocatedBytesForCurrentThread();
        for (int i = 0; i < samples; i++)
        { long start = Stopwatch.GetTimestamp(); Step(); total[i] = Us(Stopwatch.GetTimestamp() - start); }
        double allManagedAllocation = (GC.GetAllocatedBytesForCurrentThread() - allocation) / (double)samples;
        BridgeStats after = app.Stats;
        ulong callbacks = after.Callbacks - before.Callbacks;
        if (callbacks != (mode == "idle" ? 0UL : (ulong)samples)) throw new Exception("Unexpected callback count");
        int expected = 1 + count * (buttons ? 2 : 1);
        if (app.NodeCount != expected) throw new Exception($"Unexpected tree: {app.NodeCount} != {expected}");
        return new { count, widgets = selects ? "closed selects" : buttons ? "buttons" : "text", mode, nodes = app.NodeCount,
            utf8_bytes = app.Utf8Bytes, payload_bytes = app.NodeCount * sizeof(Command) + app.Utf8Bytes,
            isolated_managed_build = Distribution(buildTimes), build_alloc_bytes = buildAllocation,
            copy_validate = Distribution(copies),
            cpu_step = Distribution(total), step_managed_alloc_bytes = allManagedAllocation,
            callbacks, callback_mean_us = (after.CallbackMicroseconds - before.CallbackMicroseconds) / (double)samples,
            native_copy_validate_adopt_mean_us = (after.DecodeMicroseconds - before.DecodeMicroseconds) / (double)samples,
            native_declaration_mean_us = (after.DeclarationMicroseconds - before.DeclarationMicroseconds) / (double)samples,
            callback_managed_alloc_bytes = (after.ManagedAllocatedBytes - before.ManagedAllocatedBytes) / (double)samples,
            one_core_percent_if_forced_at_60hz = total.Average() * .006
        };
    }
    private static object RoundTrip(int samples)
    {
        int clicks = 0;
        using var app = new Application(ui => { if (ui.Button("Action").Width(100).Height(36).Clicked()) clicks++; }, virtualTime: true);
        void Step() { app.PointerDown(20, 20); app.PointerUp(20, 20); app.Step(400, 200); }
        app.Step(400, 200);
        for (int i = 0; i < 16; i++) Step();
        int before = clicks;
        double[] values = new double[samples];
        long allocation = GC.GetAllocatedBytesForCurrentThread();
        for (int i = 0; i < samples; i++)
        { long start = Stopwatch.GetTimestamp(); Step(); values[i] = Us(Stopwatch.GetTimestamp() - start); }
        long allocated = GC.GetAllocatedBytesForCurrentThread() - allocation;
        if (clicks - before != samples) throw new Exception("Click was lost or consumed twice");
        return new { widgets = "one button", mode = "pointer round trip", samples, cpu = Distribution(values),
            managed_alloc_bytes = allocated / (double)samples,
            note = "Two automation P/Invokes plus CPU Step; includes native input, event return and C# settling. Not raw FFI latency; the counter is nonvisual and the button label is stable." };
    }
}
