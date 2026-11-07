namespace Plico.Testing;

/// A manually pumped completion queue for deterministic service/task tests.
/// Pump on the creating thread; no native event loop is created.
public sealed class ManualDispatcher : IUiDispatcher, IDisposable
{
    private readonly object gate = new();
    private readonly Queue<(Action Action, TaskCompletionSource Completion, CancellationToken Cancellation)> queue = new();
    private readonly int owner = Environment.CurrentManagedThreadId;
    private bool closed;
    public int Invalidations { get; private set; }
    public void Invalidate() { Verify(); ObjectDisposedException.ThrowIf(closed, this); Invalidations++; }
    public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(action);
        if (cancellationToken.IsCancellationRequested) return Task.FromCanceled(cancellationToken);
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        lock (gate)
        {
            ObjectDisposedException.ThrowIf(closed, this);
            if (queue.Count >= 4096) throw new InvalidOperationException("Dispatcher queue exceeds 4096 operations");
            queue.Enqueue((action, completion, cancellationToken));
        }
        return completion.Task;
    }
    public void Pump()
    {
        Verify();
        (Action Action, TaskCompletionSource Completion, CancellationToken Cancellation)[] ready;
        lock (gate) { ObjectDisposedException.ThrowIf(closed, this); ready = queue.ToArray(); queue.Clear(); }
        foreach (var work in ready)
        {
            if (work.Cancellation.IsCancellationRequested) { work.Completion.TrySetCanceled(work.Cancellation); continue; }
            try { work.Action(); Invalidations++; work.Completion.TrySetResult(); }
            catch (Exception error) { work.Completion.TrySetException(error); }
        }
    }
    private void Verify()
    {
        if (Environment.CurrentManagedThreadId != owner) throw new InvalidOperationException("Pump belongs to the creating thread");
    }
    public void Dispose()
    {
        Verify();
        lock (gate)
        {
            if (closed) return;
            closed = true;
            while (queue.TryDequeue(out var work)) work.Completion.TrySetException(new ObjectDisposedException(nameof(ManualDispatcher)));
        }
    }
}
