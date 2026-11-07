namespace Plico;

/// UI-owned shader snapshot with background file reads/validation. Errors retain
/// the last good program; live reload queues through IUiDispatcher. No Rust or C#
/// compilation is invoked. Dispose before closing the owning dispatcher/app.
public sealed class WgslFile : IDisposable
{
    private readonly object gate = new();
    private readonly IUiDispatcher dispatcher;
    private readonly Func<WgslProgram, string?> validator;
    private readonly WgslMode mode;
    private readonly FileSystemWatcher? watcher;
    private CancellationTokenSource? pending;
    private long generation;
    private bool disposed;
    public string Path { get; }
    public WgslProgram? Current { get; private set; }
    public string? Error { get; private set; }
    public int Revision { get; private set; }
    public bool Watching => watcher is not null;

    /// Construct outside Show. The initial read/validation is synchronous;
    /// subsequent ReloadAsync/watcher reads run on a worker. Supply a validator
    /// for native-free tests; the default reuses the native Naga validator.
    public WgslFile(string path, IUiDispatcher dispatcher, bool watch = false, WgslMode mode = WgslMode.Effect,
        Func<WgslProgram, string?>? validator = null)
    {
        Path = System.IO.Path.GetFullPath(path);
        this.dispatcher = dispatcher ?? throw new ArgumentNullException(nameof(dispatcher));
        this.validator = validator ?? (program => program.Validate()); this.mode = mode;
        if (watch)
        {
            watcher = new FileSystemWatcher(System.IO.Path.GetDirectoryName(Path)!, System.IO.Path.GetFileName(Path))
            { NotifyFilter = NotifyFilters.LastWrite | NotifyFilters.FileName | NotifyFilters.Size };
            watcher.Changed += Changed; watcher.Created += Changed; watcher.Deleted += Changed;
            watcher.Renamed += Renamed; watcher.Error += WatchError;
            watcher.EnableRaisingEvents = true;
        }
        try { var result = Read(); lock (gate) Publish(result); }
        catch { watcher?.Dispose(); throw; }
    }
    private (WgslProgram? Program, string? Error) Read()
    {
        try
        {
            var program = WgslProgram.Load(Path, mode);
            string? error = validator(program);
            return (error is null ? program : null, error);
        }
        catch (Exception error) when (Recoverable(error)) { return (null, $"{Path}: {error.Message}"); }
    }
    private static bool Recoverable(Exception error) => error is IOException or UnauthorizedAccessException or ArgumentException or InvalidOperationException;
    private void Publish((WgslProgram? Program, string? Error) result)
    {
        Error = result.Error;
        if (result.Program is { } program && program != Current) { Current = program; Revision++; }
    }
    public Task ReloadAsync()
    {
        lock (gate)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            pending?.Cancel();
            var source = pending = new CancellationTokenSource();
            return Reload(source, ++generation);
        }
    }
    private async Task Reload(CancellationTokenSource source, long request)
    {
        try
        {
            await Task.Delay(80, source.Token).ConfigureAwait(false); // Coalesce editor save bursts.
            var result = await Task.Run(Read, source.Token).ConfigureAwait(false);
            lock (gate)
            {
                if (disposed || generation != request) return;
                if (Error == result.Error && (result.Program is null || result.Program == Current)) return;
            }
            await dispatcher.InvokeAsync(() =>
            {
                lock (gate) { if (!disposed && generation == request) Publish(result); }
            }, source.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (source.IsCancellationRequested) { }
        catch (ObjectDisposedException) { } // Dispatcher closes while work is in flight.
        finally
        {
            lock (gate) { if (ReferenceEquals(pending, source)) pending = null; }
            source.Dispose();
        }
    }
    private void Changed(object sender, FileSystemEventArgs args) => Schedule();
    private void Renamed(object sender, RenamedEventArgs args) => Schedule();
    private void WatchError(object sender, ErrorEventArgs args) => Schedule();
    private void Schedule()
    {
        lock (gate) { if (!disposed) _ = ReloadAsync(); }
    }
    public void Dispose()
    {
        lock (gate) { if (disposed) return; disposed = true; generation++; pending?.Cancel(); }
        watcher?.Dispose();
    }
}
