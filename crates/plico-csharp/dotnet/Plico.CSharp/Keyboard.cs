namespace Plico;
public sealed unsafe partial class Ui
{
    /// Unmodified keys not consumed by focused controls, IME or a modal. Bindings
    /// belong to the current container and disappear when that container unmounts.
    /// This is not a global OS hotkey or a Ctrl/Meta shortcut parser.
    public bool KeyPressed(string key)
    {
        ArgumentException.ThrowIfNullOrEmpty(key);
        if (System.Text.Encoding.UTF8.GetByteCount(key) > 64 || key.Contains('\0')) throw new ArgumentOutOfRangeException(nameof(key));
        if (parents.Count == 0) throw new InvalidOperationException("KeyPressed requires a container scope");
        int index = parents.Peek();
        var keys = Properties(index).Keys ??= new();
        if (!keys.Contains(key, StringComparer.Ordinal))
        {
            if (keys.Count >= 32) throw new InvalidOperationException("A container supports at most 32 key bindings");
            keys.Add(key);
        }
        return events.TryGetValue(commands[index].id, out var input) && input.Kind == 8 && input.Text == key;
    }
}
