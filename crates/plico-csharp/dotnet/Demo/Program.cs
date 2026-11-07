using System.Diagnostics;
using System.Text.Json;
using Plico;

var model = new Demo();
using var app = new Application(model.Show);
model.App = app;
if (args.Length >= 2 && args[0] == "--image")
{
    model.Image = app.LoadImage(args[1]);
    args = args[2..];
}
if (args.Length > 0 && args[0] == "--watch-probe")
{
    string last = "";
    var until = Stopwatch.StartNew();
    while (until.Elapsed.TotalSeconds < 60)
    {
        app.Step();
        if (model.Caption != last) { Console.WriteLine($"watch pid={Environment.ProcessId} count={model.Count} caption={model.Caption}"); last = model.Caption; }
        Thread.Sleep(30);
    }
}
else if (args.Length > 0 && args[0] == "--self-test")
{
    ComponentChecks.Run();
    LocalStateChecks.Run();
    SdkChecks.Run();
    PrimitiveChecks.Run();
    FrameworkWidgetChecks.Run();
    IdentityChecks.Run();
    app.Step();
    Require(model.Count == 0, "initial count");
    ulong initialCallbacks = app.Stats.Callbacks;
    for (int i = 0; i < 10; i++) app.Step();
    Require(app.Stats.Callbacks == initialCallbacks, "native frames replay the owned batch without entering C#");
    app.Click("button", "Increment"); app.Step();
    Require(model.Count == 1, "click updates ordinary C# state");
    Require(model.LastLabel == "Count: 1", "label before handler settles to the new state");
    app.Click("checkbox", "Show details"); app.Step();
    Require(!model.Details, "checkbox event updates ref field");
    app.Click("button", "Increment"); app.Step();
    Require(model.Count == 2, "conditional subtree removal preserves button identity");
    app.SetText("Translation", "中文・日本語・emoji 👩‍💻"); app.Step();
    Require(model.Translation == "中文・日本語・emoji 👩‍💻", "Unicode input crosses the batch event channel");
    app.SetValue("Font size", 36); app.Step();
    Require(model.FontSize == 36, "slider event crosses the batch channel");
    GC.Collect(); GC.WaitForPendingFinalizers();
    model.Count = 42; app.Invalidate(); app.Step();
    Require(model.LastLabel == "Count: 42", "external mutation plus explicit invalidation");
    app.Click("button", "Reset"); app.Step();
    Require(model.Count == 0, "reset after forced GC");
    using (var grown = new Application(ui =>
    {
        using (ui.Column().FillWidth().FillHeight().ScrollY())
            for (int i = 0; i < 512; i++) ui.Text($"长文本，第 {i} 行，测试批次缓冲区增长。", key: i.ToString());
    }))
    {
        grown.Step(); GC.Collect(); grown.Invalidate(); grown.Step();
        Require(grown.NodeCount == 513 && grown.Utf8Bytes > 4096, "pinned arrays grow safely and survive GC");
    }
    bool rejectedWrongThread = Task.Run(() =>
    {
        try { app.Step(); return false; } catch (InvalidOperationException) { return true; }
    }).GetAwaiter().GetResult();
    Require(rejectedWrongThread, "host is thread-affine");
    int dispatcherThread = 0;
    Task? applied = null;
    Task.Run(() =>
    {
        applied = app.InvokeAsync(() =>
        {
            dispatcherThread = Environment.CurrentManagedThreadId;
            model.Count = 71;
        });
    }).GetAwaiter().GetResult();
    Require(!applied!.IsCompleted, "worker only queues, never executes UI state updates");
    app.Step();
    applied.GetAwaiter().GetResult();
    Require(dispatcherThread == Environment.CurrentManagedThreadId && model.LastLabel == "Count: 71",
        "queued work executes before the UI declaration on its owner thread");
    var failure = app.InvokeAsync(() => throw new ArgumentException("dispatch failure"));
    var success = app.InvokeAsync(() => model.Count = 72);
    app.Step();
    Require(failure.IsFaulted && success.IsCompletedSuccessfully && model.Count == 72,
        "a failing operation does not poison the dispatcher");
    using (var cancellation = new CancellationTokenSource())
    {
        var cancelled = app.InvokeAsync(() => model.Count = -1, cancellation.Token);
        cancellation.Cancel(); app.Step();
        Require(cancelled.IsCanceled && model.Count == 72, "cancelled queued work does not mutate state");
    }
    app.Post(() => { model.Count = 73; app.Post(() => model.Count = 74); });
    app.Step();
    Require(model.Count == 73, "self-posted work waits for the next build");
    app.Step();
    Require(model.Count == 74, "a wake during the drain is not lost");
    model.Count = 75;
    HotReload.UpdateApplication(null); app.Step();
    Require(model.LastLabel == "Count: 75", "metadata update refreshes cached declarations without resetting state");
    string imagePath = Path.Combine(Path.GetTempPath(), $"plico-csharp-{Guid.NewGuid()}.png");
    try
    {
        File.WriteAllBytes(imagePath, Convert.FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="));
        ulong loaded = app.LoadImageAsync(imagePath).GetAwaiter().GetResult();
        var assignImage = app.InvokeAsync(() => model.Image = loaded);
        app.Step();
        Require(assignImage.IsCompletedSuccessfully && model.Image != 0,
            "worker resource registration becomes available in the next batch");
    }
    finally { File.Delete(imagePath); }
    var closed = new Application(ui => ui.Text("temporary"));
    var abandoned = closed.InvokeAsync(() => { });
    closed.Dispose();
    Require(abandoned.IsFaulted, "closing completes queued tasks");
    bool rejectedClosed = false;
    try { closed.Post(() => { }); } catch (ObjectDisposedException) { rejectedClosed = true; }
    Require(rejectedClosed, "closed hosts reject new work");
    using (var broken = new Application(_ => throw new InvalidOperationException("intentional managed error")))
    {
        bool caught = false;
        try { broken.Step(); } catch (InvalidOperationException) { caught = true; }
        Require(caught, "managed exceptions become native diagnostics");
    }
    app.Step();
    Console.WriteLine(JsonSerializer.Serialize(new DemoResult("passed", app.NodeCount, app.Utf8Bytes, app.Stats), DemoJsonContext.Default.DemoResult));
}
else if (args.Length > 0 && args[0] == "--bench")
{
    for (int i = 0; i < 20; i++) { model.Count = i; app.Invalidate(); app.Step(); }
    var before = app.Stats; var watch = Stopwatch.StartNew();
    for (int i = 0; i < 500; i++) { model.Count = i; app.Invalidate(); app.Step(); }
    watch.Stop(); var after = app.Stats;
    Console.WriteLine(JsonSerializer.Serialize(new BenchResult(500,
        watch.Elapsed.TotalMicroseconds / 500,
        (after.CallbackMicroseconds - before.CallbackMicroseconds) / 500.0,
        (after.DecodeMicroseconds - before.DecodeMicroseconds) / 500.0,
        (after.DeclarationMicroseconds - before.DeclarationMicroseconds) / 500.0,
        (after.ManagedAllocatedBytes - before.ManagedAllocatedBytes) / 500.0,
        app.NodeCount, app.Utf8Bytes, "CPU-only forced updates; no GPU submission"), DemoJsonContext.Default.BenchResult));
}
else if (args.Length >= 2 && args[0] == "--capture")
{
    app.Capture(args[1]);
    Console.WriteLine($"Captured {args[1]}");
}
else app.Run();

static void Require(bool condition, string message)
{
    if (!condition) throw new InvalidOperationException("Self-test: " + message);
}

sealed class Demo
{
    public int Count;
    private int tracedCount = int.MinValue;
    private readonly bool trace = Environment.GetEnvironmentVariable("PLICO_DEMO_TRACE") == "1";
    public bool Details = true;
    public double FontSize = 28;
    public string Translation = "在 C# 中构建界面，使用 Plico 的系统文字与 wgpu 渲染。";
    public string LastLabel = "";
    public ulong Image;
    public Application App = null!;
    public string Caption = "";
    public string Status = "Ready";
    private string tracedStatus = "";
    public bool Busy;
    private async Task SyncAsync()
    {
        try
        {
            await Task.Delay(300); // A network request or other asynchronous service.
            await App.InvokeAsync(() => { Status = "Synced"; Busy = false; });
        }
        catch (ObjectDisposedException) { } // The window closed before completion.
    }
    public void Show(Ui ui)
    {
        if (trace && tracedCount != Count) { Console.WriteLine($"count={Count}"); tracedCount = Count; }
        if (trace && tracedStatus != Status) { Console.WriteLine($"status={Status}"); tracedStatus = Status; }
        using (ui.Column().FillWidth().FillHeight().Padding(32).Gap(20).Background(0xffffff))
        {
            Caption = "Plico, from C#";
            ui.Text(Caption).FontSize(28).Weight(700);
            ui.Text("One typed batch. Native text, layout and rendering.").Color(0x65656f);
            using (ui.Row().FillWidth().Gap(24).AlignStart())
            {
                using (ui.Column().Width(380).Gap(16))
                {
                    ui.Text("Counter").FontSize(20).Weight(600);
                    LastLabel = $"Count: {Count}";
                    ui.Text(LastLabel).FontSize(36).Weight(650);
                    using (ui.Row().Gap(8))
                    {
                        if (ui.Button("Increment").Primary().Clicked()) Count++;
                        if (ui.Button("Decrement").Ghost().Clicked()) Count--;
                        if (ui.Button("Reset").Clicked()) Count = 0;
                    }
                    using (ui.Row().Gap(8).AlignCenter())
                    {
                        if (ui.Button("Sync asynchronously").Disabled(Busy).Clicked())
                        {
                            Busy = true; Status = "Syncing…"; _ = SyncAsync();
                        }
                        ui.Text(Status);
                    }
                    ui.Checkbox("Show details", ref Details);
                    if (Details)
                    {
                        using (ui.Column("details").Padding(16).Gap(8).Background(0xf4f4f5).Radius(8))
                        {
                            ui.Text("Ordinary fields, ordinary if/for.").Weight(600);
                            ui.Text("No signal wrapper or borrowed callback state.").Color(0x65656f);
                        }
                    }
                    ui.Text("Translation").Weight(600);
                    ui.TextInput("Translation", ref Translation).FillWidth();
                    ui.Text("Font size").Weight(600);
                    ui.Slider("Font size", ref FontSize, 12, 48).FillWidth();
                }
                using (ui.Column().Width(0).Grow().Padding(24).Gap(20).Background(0xf9f9fa).Radius(12))
                {
                    ui.Text("Live preview").FontSize(18).Weight(600);
                    ui.Shader("""
                        fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
                            let time = plico.resolution_time_scale.z;
                            let wave = 0.5 + 0.5 * sin(uv.x * 7.0 + time);
                            let color = mix(vec3<f32>(0.86, 0.92, 0.98), vec3<f32>(0.98, 0.86, 0.90), wave);
                            return vec4<f32>(color * (0.94 + 0.06 * uv.y), 1.0);
                        }
                        """, animated: true).FillWidth().Height(80).Radius(10);
                    ui.Text(Translation).FontSize((uint)FontSize).FillWidth();
                    if (Image != 0) ui.Image(Image, 200, 280);
                    using (ui.Row().Gap(8))
                    {
                        ui.Spinner();
                        ui.Text("Native animation; no C# frame rebuild.").Color(0x65656f);
                    }
                }
            }
        }
    }
}
