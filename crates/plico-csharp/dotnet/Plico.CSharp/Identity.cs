using System.Runtime.CompilerServices;

namespace Plico;

public sealed unsafe partial class Ui
{
    private int ownerThread, generation;
    private bool building;
    private ulong currentScope;
    private readonly Stack<ulong> scopes = new();
    private readonly HashSet<ulong> seen = new(), seenScopes = new();
    private readonly Dictionary<(string File, int Line), ulong> callSites = new();

    internal void EndBuild() { EndLocalStateBuild(); building = false; }

    private bool Valid(int expectedGeneration) => building && generation == expectedGeneration &&
        Environment.CurrentManagedThreadId == ownerThread;
    private void Verify(int expectedGeneration)
    {
        if (!Valid(expectedGeneration))
            throw new InvalidOperationException("UI builders are only valid on the UI thread during their original build");
    }
    private static ulong Mix(ulong left, ulong right) =>
        unchecked((left ^ right) * 1099511628211UL);
    private ulong Caller(string file, int line)
    {
        var site = (file, line);
        if (callSites.TryGetValue(site, out ulong hash)) return hash;
        // Bound metadata across long hot-reload sessions with moving source locations.
        if (callSites.Count >= 4096) callSites.Clear();
        hash = Mix(Hash(file, 14695981039346656037UL), (uint)line);
        callSites.Add(site, hash);
        return hash;
    }
    private ulong Address(uint kind, string file, int line, string? key)
    {
        ulong parent = parents.Count == 0 ? 14695981039346656037UL : commands[parents.Peek()].id;
        ulong hash = Mix(Mix(parent, currentScope), kind);
        if (key is not null) hash = Mix(hash, Hash(key, 14695981039346656037UL));
        else
        {
            hash = Mix(hash, Caller(file, line));
            occurrences.TryGetValue(hash, out int occurrence);
            occurrences[hash] = occurrence + 1;
            hash = Mix(hash, (uint)occurrence);
        }
        return hash == 0 ? 1 : hash;
    }

    /// Identity only: no layout node, callback or retained component object.
    public IdentityScope Scope([CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        Verify(generation);
        return EnterScope(Address(100, file, line, null));
    }
    public IdentityScope Scope(string key)
    {
        ArgumentNullException.ThrowIfNull(key);
        Verify(generation);
        return EnterScope(Address(101, "", 0, key));
    }
    public IdentityScope Scope(long key)
    {
        Verify(generation);
        return EnterScope(Mix(Address(102, "", 0, ""), unchecked((ulong)key)));
    }
    public IdentityScope Scope(ulong key)
    {
        Verify(generation);
        return EnterScope(Mix(Address(104, "", 0, ""), key));
    }
    public IdentityScope Scope(Guid key)
    {
        Verify(generation);
        Span<byte> bytes = stackalloc byte[16];
        key.TryWriteBytes(bytes);
        ulong hash = Address(103, "", 0, "");
        foreach (byte value in bytes) hash = Mix(hash, value);
        return EnterScope(hash);
    }
    private IdentityScope EnterScope(ulong id)
    {
        if (!seenScopes.Add(id)) throw new InvalidOperationException("Duplicate identity Scope; use unique business IDs");
        return new(this, id);
    }
    public readonly struct IdentityScope : IDisposable
    {
        private readonly Ui ui;
        private readonly ulong previous, id;
        private readonly int generation, parentDepth, depth;
        internal IdentityScope(Ui ui, ulong id)
        {
            this.ui = ui; this.id = id; previous = ui.currentScope;
            generation = ui.generation; parentDepth = ui.parents.Count;
            ui.scopes.Push(id); depth = ui.scopes.Count; ui.currentScope = id;
        }
        public void Dispose()
        {
            ui.Verify(generation);
            if (ui.scopes.Count != depth || ui.scopes.Peek() != id || ui.parents.Count != parentDepth)
                throw new InvalidOperationException("Identity and layout scopes must close in nesting order");
            ui.scopes.Pop(); ui.currentScope = previous;
        }
    }
}
