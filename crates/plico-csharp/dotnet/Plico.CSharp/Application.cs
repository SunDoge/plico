using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using Plico.Interop;

namespace Plico;

public readonly record struct BridgeStats(ulong Callbacks, ulong Batches, ulong Declarations, ulong Commands, ulong CallbackMicroseconds, ulong DecodeMicroseconds, ulong DeclarationMicroseconds, long ManagedAllocatedBytes);

public sealed unsafe class Application : IDisposable
{
    private ulong handle;
    private readonly object dispatchGate = new();
    private readonly Queue<DispatchWork> dispatchQueue = new();
    private bool closed;
    private sealed record DispatchWork(Action Action, TaskCompletionSource? Completion, CancellationToken Cancellation);

    private readonly int ownerThread = Environment.CurrentManagedThreadId;
    private bool inBuild, running;
    private GCHandle root;
    private readonly Action<Ui> build;
    private readonly Ui ui = new();
    private long allocatedBytes;
    /// Borrowed integration token for native service plugins; it does not transfer ownership.
    [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Never)]
    public ulong NativeHandle { get { EnsureThread(); ObjectDisposedException.ThrowIf(closed, this); return handle; } }
    public int NodeCount => ui.NodeCount;
    public int Utf8Bytes => ui.ByteCount;
    static Application()
    {
        NativeLibrary.SetDllImportResolver(typeof(Application).Assembly, (name, assembly, searchPath) =>
        {
            string? path = Environment.GetEnvironmentVariable("PLICO_NATIVE_LIBRARY");
            return name == "plico_csharp" && path is not null ? NativeLibrary.Load(Path.GetFullPath(path)) : IntPtr.Zero;
        });
    }
    public Application(Action<Ui> build, bool virtualTime = false)
    {
        this.build = build;
        if (Native.plico_abi_version() != 5 || Native.plico_command_size() != sizeof(Command))
            throw new InvalidOperationException("Plico native/managed ABI mismatch; rebuild the native library and bindings");
        root = GCHandle.Alloc(this);
        handle = virtualTime
            ? Native.plico_create_virtual(&Build, (void*)GCHandle.ToIntPtr(root))
            : Native.plico_create(&Build, (void*)GCHandle.ToIntPtr(root));
        if (handle == 0) { root.Free(); throw new InvalidOperationException(Error()); }
        HotReload.Register(this);
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static int Build(void* context, Frame* frame, Batch* batch)
    {
        try
        {
            var app = (Application)GCHandle.FromIntPtr((IntPtr)context).Target!;
            long before = GC.GetAllocatedBytesForCurrentThread();
            app.inBuild = true;
            try
            {
                app.DrainPosted();
                app.ui.Begin(frame, acceptEvents: true);
                app.build(app.ui);
                if (frame->event_count != 0)
                {
                    // Inline handlers may run after labels were declared. Rebuild
                    // once without consuming the event again, to submit coherent UI.
                    app.ui.Begin(frame, acceptEvents: false);
                    app.build(app.ui);
                }
                *batch = app.ui.Finish();
            }
            finally { app.ui.EndBuild(); app.inBuild = false; }
            app.allocatedBytes += GC.GetAllocatedBytesForCurrentThread() - before;
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine(error);
            return -1; // Never unwind a managed exception through native code.
        }
    }
    /// Queue a short state update from any thread. Runs before the next UI build.
    /// Exceptions are logged; use InvokeAsync to observe failure.
    public void Post(Action action) => Enqueue(action, null, default);

    /// Completion means the state update executed, not that pixels were presented.
    /// Never synchronously wait for this task on the UI thread.
    public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(action);
        if (cancellationToken.IsCancellationRequested) return Task.FromCanceled(cancellationToken);
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        Enqueue(action, completion, cancellationToken);
        return completion.Task;
    }
    private void Enqueue(Action action, TaskCompletionSource? completion, CancellationToken cancellation)
    {
        ArgumentNullException.ThrowIfNull(action);
        lock (dispatchGate)
        {
            ObjectDisposedException.ThrowIf(closed, this);
            if (dispatchQueue.Count >= 4096) throw new InvalidOperationException("UI dispatch queue exceeds 4096 operations");
            // Wake before enqueue is safe: the UI drain must acquire this same gate.
            Check(Native.plico_wake(handle));
            dispatchQueue.Enqueue(new(action, completion, cancellation));
        }
    }
    private void DrainPosted()
    {
        DispatchWork[] work;
        lock (dispatchGate) { work = dispatchQueue.ToArray(); dispatchQueue.Clear(); }
        // Snapshot prevents a producer or a self-posting action monopolizing this frame.
        foreach (var item in work)
        {
            if (item.Cancellation.IsCancellationRequested)
            {
                item.Completion?.TrySetCanceled(item.Cancellation);
                continue;
            }
            try { item.Action(); item.Completion?.TrySetResult(); }
            catch (Exception error)
            {
                if (item.Completion is not null) item.Completion.TrySetException(error);
                else Console.Error.WriteLine(error);
            }
        }
    }
    private void CloseDispatcher()
    {
        lock (dispatchGate)
        {
            closed = true;
            HotReload.Unregister(this);
            while (dispatchQueue.TryDequeue(out var item))
                item.Completion?.TrySetException(new ObjectDisposedException(nameof(Application)));
        }
    }
    /// Loads/validates an image on a worker. Assign its ID inside InvokeAsync.
    public Task<ulong> LoadImageAsync(string path, CancellationToken cancellationToken = default)
    {
        string fullPath = Path.GetFullPath(path);
        return Task.Run(() => LoadImage(fullPath), cancellationToken);
    }
    /// Setup-time synchronous loading. Use LoadImageAsync while the app is running.
    public ulong LoadImage(string path)
    {
        ulong resourceHandle;
        lock (dispatchGate)
        {
            ObjectDisposedException.ThrowIf(closed, this);
            resourceHandle = handle;
        }
        byte[] bytes = Encoding.UTF8.GetBytes(Path.GetFullPath(path));
        fixed (byte* ptr = bytes)
        {
            ulong resource = Native.plico_load_image(resourceHandle, ptr, (uint)bytes.Length);
            if (resource == 0) throw new InvalidOperationException(Error());
            return resource;
        }
    }
    public (double X, double Y, double Width, double Height, double Scale) CanvasViewport
    {
        get
        {
            EnsureThread(); Plico.Interop.CanvasViewport value;
            Check(Native.plico_canvas_viewport(handle, &value));
            return (value.x, value.y, value.width, value.height, value.scale);
        }
    }
    public void PointerMove(double x, double y, bool pressed = false) { EnsureThread(); Check(Native.plico_test_pointer(handle, 0, x, y, pressed ? 1U : 0U)); }
    public void PointerDown(double x, double y) { EnsureThread(); Check(Native.plico_test_pointer(handle, 1, x, y, 1)); }
    public void PointerUp(double x, double y) { EnsureThread(); Check(Native.plico_test_pointer(handle, 2, x, y, 0)); }

    /// Transfers ownership of an image ID returned by a native service.
    public ImageResource AdoptImageResource(ulong resource, bool canvasOnly = false)
    {
        EnsureThread(); ObjectDisposedException.ThrowIf(closed, this);
        if (resource == 0) throw new ArgumentOutOfRangeException(nameof(resource));
        return new(this, resource, canvasOnly);
    }
    public Task<ImageResource> LoadImageResourceAsync(string path, CancellationToken cancellationToken = default) =>
        ImageResource.LoadAsync(this, path, cancellationToken);
    internal void ReleaseImageResource(ulong resource)
    {
        lock (dispatchGate) { if (!closed) Check(Native.plico_release_image(handle, resource)); }
    }

    public void FitCanvas() { EnsureThread(); Check(Native.plico_canvas_control(handle, 0, 0)); }
    public void ActualSizeCanvas() { EnsureThread(); Check(Native.plico_canvas_control(handle, 1, 0)); }
    public void ZoomCanvas(double factor) { EnsureThread(); Check(Native.plico_canvas_control(handle, 2, factor)); }

    public void Invalidate() { EnsureThread(); lock (dispatchGate) { ObjectDisposedException.ThrowIf(closed, this); Check(Native.plico_wake(handle)); } }
    /// Advance an explicitly virtual host to an absolute, monotonic time.
    public void SetTime(ulong milliseconds) { EnsureThread(); Check(Native.plico_set_time(handle, milliseconds)); }
    public void KeyPress(string key)
    {
        EnsureThread(); byte[] bytes = Encoding.UTF8.GetBytes(key);
        fixed (byte* p = bytes) Check(Native.plico_test_key(handle, p, (uint)bytes.Length));
    }
    public void Scroll(double x, double y, double deltaY) { EnsureThread(); Check(Native.plico_test_scroll(handle, x, y, deltaY)); }
    public void Step(uint width = 960, uint height = 720) { EnsureThread(); Check(Native.plico_test_frame(handle, width, height)); }
    public void SetText(string label, string value)
    {
        EnsureThread(); byte[] l = Encoding.UTF8.GetBytes(label), t = Encoding.UTF8.GetBytes(value);
        fixed (byte* lp = l) fixed (byte* tp = t) Check(Native.plico_test_text(handle, lp, (uint)l.Length, tp, (uint)t.Length));
    }
    public void SetValue(string label, double value)
    {
        EnsureThread(); byte[] l = Encoding.UTF8.GetBytes(label);
        fixed (byte* lp = l) Check(Native.plico_test_value(handle, lp, (uint)l.Length, value));
    }
    public void Click(string role, string label)
    {
        EnsureThread();
        byte[] r = Encoding.UTF8.GetBytes(role), l = Encoding.UTF8.GetBytes(label);
        fixed (byte* rp = r) fixed (byte* lp = l)
            Check(Native.plico_test_click(handle, rp, (uint)r.Length, lp, (uint)l.Length));
    }
    public void Capture(string path, uint width = 960, uint height = 720)
    {
        EnsureThread();
        path = Path.GetFullPath(path); Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        byte[] bytes = Encoding.UTF8.GetBytes(path);
        fixed (byte* ptr = bytes) Check(Native.plico_capture(handle, ptr, (uint)bytes.Length, width, height));
    }
    public void Run(string title = "Plico × C#", uint width = 1040, uint height = 780) =>
        Run(new WindowOptions { Title = title, Width = width, Height = height });
    public void Run(WindowOptions options)
    {
        ArgumentNullException.ThrowIfNull(options);
        EnsureThread();
        if (inBuild || running) throw new InvalidOperationException("Cannot recursively run a native host");
        byte[] bytes = options.Encode(); // Validation precedes consuming the host.
        running = true;
        try { fixed (byte* ptr = bytes) Check(Native.plico_run_options(handle, ptr, (uint)bytes.Length)); }
        finally { running = false; CloseDispatcher(); ui.ClearLocalStates(); Native.plico_destroy(handle); handle = 0; GC.KeepAlive(this); }
    }
    public BridgeStats Stats
    {
        get
        {
            EnsureThread();
            Stats stats; Check(Native.plico_stats(handle, &stats));
            return new(stats.callbacks, stats.batches, stats.declarations, stats.commands, stats.callback_micros, stats.decode_micros, stats.declare_micros, allocatedBytes);
        }
    }
    private static void Check(int result) { if (result != 1) throw new InvalidOperationException(Error()); }
    private static string Error()
    {
        uint length = Native.plico_last_error(null, 0);
        var bytes = new byte[checked((int)length)];
        fixed (byte* ptr = bytes) Native.plico_last_error(ptr, length);
        return Encoding.UTF8.GetString(bytes);
    }
    private void EnsureThread()
    {
        if (Environment.CurrentManagedThreadId != ownerThread) throw new InvalidOperationException("Plico host must be used on its creating thread");
    }
    public void Dispose()
    {
        EnsureThread();
        if (inBuild || running) throw new InvalidOperationException("Cannot dispose a running native host or a build callback");
        CloseDispatcher();
        ui.ClearLocalStates();
        if (handle != 0) { Native.plico_destroy(handle); handle = 0; }
        if (root.IsAllocated) root.Free();
        GC.SuppressFinalize(this);
    }
}
