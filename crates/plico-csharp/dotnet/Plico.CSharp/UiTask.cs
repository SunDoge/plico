namespace Plico;

public enum UiTaskStatus { Idle, Running, Succeeded, Failed, Cancelled }

/// <summary>Application-owned task snapshot. Start/Cancel/Dispose on the UI thread.</summary>
public sealed class UiTask<T>(Application application) : IDisposable
{
    private readonly int ownerThread = Environment.CurrentManagedThreadId;
    private CancellationTokenSource? cancellation;
    private int version;
    private bool disposed;
    public UiTaskStatus Status { get; private set; }
    public T? Result { get; private set; }
    public Exception? Error { get; private set; }
    public Task Completion { get; private set; } = Task.CompletedTask;
    public void Start(Func<CancellationToken, Task<T>> operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        Verify(); ObjectDisposedException.ThrowIf(disposed, this);
        cancellation?.Cancel();
        var source = new CancellationTokenSource();
        cancellation = source;
        int current = ++version;
        Status = UiTaskStatus.Running; Result = default; Error = null;
        application.Invalidate();
        Completion = Execute(operation, source, current);
    }
    private async Task Execute(Func<CancellationToken, Task<T>> operation, CancellationTokenSource source, int current)
    {
        T? result = default; Exception? error = null;
        bool cancelled = false;
        try { result = await Task.Run(() => operation(source.Token), source.Token).ConfigureAwait(false); }
        catch (OperationCanceledException) when (source.IsCancellationRequested) { cancelled = true; }
        catch (Exception exception) { error = exception; }
        try
        {
            await application.InvokeAsync(() =>
            {
                if (disposed || version != current) return;
                cancellation = null;
                Result = result; Error = error;
                Status = cancelled ? UiTaskStatus.Cancelled : error is not null ? UiTaskStatus.Failed : UiTaskStatus.Succeeded;
            }).ConfigureAwait(false);
        }
        catch (ObjectDisposedException) { } // App closes while a completion is in flight.
        finally { source.Dispose(); }
    }
    public void Cancel()
    {
        Verify();
        cancellation?.Cancel(); cancellation = null; ++version;
        if (Status == UiTaskStatus.Running)
        {
            Status = UiTaskStatus.Cancelled;
            if (!disposed) application.Invalidate();
        }
    }
    private void Verify()
    {
        if (Environment.CurrentManagedThreadId != ownerThread)
            throw new InvalidOperationException("Task controls and snapshots belong to the UI thread");
    }
    public void Dispose()
    {
        Verify();
        if (disposed) return;
        disposed = true; Cancel();
    }
}
