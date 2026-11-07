using System.ComponentModel;
using System.Reflection.Metadata;

[assembly: MetadataUpdateHandler(typeof(Plico.HotReload))]

namespace Plico;

/// <summary>.NET metadata update hook; application state and the native host stay alive.</summary>
[EditorBrowsable(EditorBrowsableState.Never)]
public static class HotReload
{
    private static readonly object gate = new();
    private static readonly List<WeakReference<Application>> applications = new();

    internal static void Register(Application application)
    {
        lock (gate)
        {
            applications.RemoveAll(reference => !reference.TryGetTarget(out _));
            applications.Add(new(application));
        }
    }
    internal static void Unregister(Application application)
    {
        lock (gate) applications.RemoveAll(reference =>
            !reference.TryGetTarget(out var target) || ReferenceEquals(target, application));
    }
    public static void ClearCache(Type[]? updatedTypes) { }
    public static void UpdateApplication(Type[]? updatedTypes)
    {
        Application[] live;
        lock (gate)
            live = applications.Select(reference => reference.TryGetTarget(out var target) ? target : null)
                .OfType<Application>().ToArray();
        foreach (var application in live)
        {
            try { application.Post(static () => { }); }
            catch (ObjectDisposedException) { } // A window may close during the update.
        }
    }
}
