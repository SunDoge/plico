using Plico;
using System.Text.Json.Serialization;
namespace Gallery;

internal sealed record GalleryResult(string result, int pages, BridgeStats stats);
[JsonSourceGenerationOptions(PropertyNameCaseInsensitive = true, UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow)]
[JsonSerializable(typeof(Scene))]
[JsonSerializable(typeof(GalleryResult))]
internal partial class GalleryJsonContext : JsonSerializerContext { }
