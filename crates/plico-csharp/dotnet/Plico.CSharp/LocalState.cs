using System.Runtime.CompilerServices;
namespace Plico;

public sealed unsafe partial class Ui
{
    // Individually allocated cells keep returned refs stable if the dictionary grows.
    // Only the value is retained: the initializer delegate is never stored.
    private sealed class StateCell<T>(T value) { internal T Value = value; }
    private readonly Dictionary<ulong, object> localStates = new();
    private readonly HashSet<ulong> liveLocalStates = new();
    private readonly HashSet<ulong> newLocalStates = new();
    private readonly Dictionary<ulong, object> replacedLocalStates = new();
    private bool localStatesCommitted;

    /// Component-local ordinary state. Use `ref var value = ref ui.State(...)`:
    /// assigning without `ref` copies value types. Identity follows the parent,
    /// Scope and call site, or an explicit key. Unmounted slots are forgotten.
    /// The returned ref is for synchronous UI code; do not retain it across builds.
    public ref T State<T>(Func<T> create, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        Verify(generation);
        ArgumentNullException.ThrowIfNull(create);
        ulong id = Address(200, file, line, key);
        if (!liveLocalStates.Add(id)) throw new InvalidOperationException("Duplicate local state key; use a unique Scope or key");
        if (localStates.TryGetValue(id, out var previous) && previous is StateCell<T> existing)
            return ref existing.Value;
        var cell = new StateCell<T>(create()); // A failing factory must not replace the previous value.
        if (previous is null) newLocalStates.Add(id);
        else if (!newLocalStates.Contains(id)) replacedLocalStates.TryAdd(id, previous);
        localStates[id] = cell;
        return ref cell.Value;
    }
    private void BeginLocalStatePass()
    {
        liveLocalStates.Clear(); localStatesCommitted = false;
    }
    private void CommitLocalStates()
    {
        Prune(localStates, liveLocalStates);
        localStatesCommitted = true;
    }
    private void EndLocalStateBuild()
    {
        if (!localStatesCommitted)
        {
            // Failed builds do not unmount existing components. Discard cells
            // initialized only by the rejected build, and restore replaced types.
            foreach (ulong id in newLocalStates) localStates.Remove(id);
            foreach (var pair in replacedLocalStates) localStates[pair.Key] = pair.Value;
        }
        newLocalStates.Clear(); replacedLocalStates.Clear();
    }
    internal void ClearLocalStates()
    {
        localStates.Clear(); liveLocalStates.Clear(); newLocalStates.Clear(); replacedLocalStates.Clear();
    }
}
