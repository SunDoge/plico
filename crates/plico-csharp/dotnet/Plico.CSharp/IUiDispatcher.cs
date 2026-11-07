namespace Plico;

/// Managed completion boundary. Services return short model updates through it;
/// they do not borrow Ui or require a native window to be tested.
public interface IUiDispatcher
{
    void Invalidate();
    Task InvokeAsync(Action action, CancellationToken cancellationToken = default);
}
