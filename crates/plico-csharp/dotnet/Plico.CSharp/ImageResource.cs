namespace Plico;

/// <summary>Application ownership of a native image. The current UI batch retains its own use.</summary>
public sealed class ImageResource : IDisposable
{
    private readonly Application owner;
    private ulong id;
    public bool CanvasOnly { get; }
    public ulong Id { get { ulong value = Volatile.Read(ref id); return value != 0 ? value : throw new ObjectDisposedException(nameof(ImageResource)); } }
    internal ImageResource(Application owner, ulong id, bool canvasOnly)
    { this.owner = owner; this.id = id; CanvasOnly = canvasOnly; }
    internal static async Task<ImageResource> LoadAsync(Application app, string path, CancellationToken token)
    {
        ulong resource = await app.LoadImageAsync(path, token).ConfigureAwait(false);
        return new ImageResource(app, resource, false);
    }
    public void Dispose()
    {
        ulong resource = Interlocked.Exchange(ref id, 0);
        if (resource != 0) owner.ReleaseImageResource(resource);
    }
}
