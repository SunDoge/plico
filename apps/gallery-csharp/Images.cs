using Plico;
using System.IO.Compression;
using System.Buffers.Binary;

namespace Gallery;

internal sealed partial class GalleryModel
{
    internal UiTask<ulong> ImageRequest = null!;
    internal string ImagePath = "", ImageUrl = "https://www.gstatic.com/webp/gallery/1.webp";
    internal double ImageWidth = 420;
    internal void StartSample()
    {
        ImageRequest.Start(async token =>
        {
            string file = Path.Combine(Path.GetTempPath(), $"plico-gallery-{Guid.NewGuid()}.png");
            try
            {
                await File.WriteAllBytesAsync(file, SamplePng.Create(), token);
                return await App.LoadImageAsync(file, token);
            }
            finally { if (File.Exists(file)) File.Delete(file); }
        });
    }
    private void Images(Ui ui)
    {
        using (Card(ui, Tr("Image resources", "图片资源")))
        {
            using (ui.Row().Gap(10).Wrap())
            {
                if (ui.Button("Load sample").Primary().Clicked()) StartSample();
                if (ImageRequest.Status == UiTaskStatus.Running) ui.Spinner();
                ui.Text(ImageRequest.Status.ToString()).Color(T.Muted);
            }
            using (ui.Row().FillWidth().Gap(10))
            {
                ui.TextInput("Image path", ref ImagePath).Width(0).Grow();
                if (ui.Button("Open file").Disabled(string.IsNullOrWhiteSpace(ImagePath)).Clicked())
                {
                    string path = ImagePath;
                    ImageRequest.Start(token => App.LoadImageAsync(path, token));
                }
            }
            using (ui.Row().FillWidth().Gap(10))
            {
                ui.TextInput("Image URL", ref ImageUrl).Width(0).Grow();
                if (ui.Button("Download").Clicked())
                {
                    string url = ImageUrl;
                    ImageRequest.Start(async token =>
                    {
                        using var client = new HttpClient { Timeout = TimeSpan.FromSeconds(20), MaxResponseContentBufferSize = 12 * 1024 * 1024 };
                        byte[] bytes = await client.GetByteArrayAsync(url, token);
                        string file = Path.Combine(Path.GetTempPath(), $"plico-download-{Guid.NewGuid()}");
                        try { await File.WriteAllBytesAsync(file, bytes, token); return await App.LoadImageAsync(file, token); }
                        finally { if (File.Exists(file)) File.Delete(file); }
                    });
                }
            }
            if (ImageRequest.Status == UiTaskStatus.Failed)
                ui.Text(ImageRequest.Error?.Message ?? "Image load failed").Color(0xdc2626).FillWidth();
            if (ImageRequest.Status == UiTaskStatus.Succeeded && ImageRequest.Result != 0)
            {
                using (ui.Column().FillWidth().Padding(16).Radius(8).Background(T.Panel))
                    ui.Image(ImageRequest.Result, (float)ImageWidth, (float)ImageWidth * 320 / 512);
                ui.Slider("Image size", ref ImageWidth, 160, Math.Max(180, ui.Width - 360)).FillWidth();
                Hint(ui, "Only the resource ID is submitted with the UI. Pixel data stays in the Rust image resource.");
            }
            else Hint(ui, "Load the bundled procedural sample, a local file, or a remote PNG/JPEG/WebP.");
        }
    }
}

// Deterministic image-pipeline fixture. No graphics framework or platform codec dependency.
internal static class SamplePng
{
    internal static byte[] Create()
    {
        const int width = 512, height = 320;
        using var raw = new MemoryStream();
        for (int y = 0; y < height; y++)
        {
            raw.WriteByte(0);
            for (int x = 0; x < width; x++)
            {
                bool ridge = y > 190 + 25 * Math.Sin(x / 58.0);
                bool sun = (x - 370) * (x - 370) + (y - 85) * (y - 85) < 34 * 34;
                raw.WriteByte((byte)(sun ? 249 : ridge ? 48 + y / 9 : 180 + y / 7));
                raw.WriteByte((byte)(sun ? 178 : ridge ? 97 + x / 16 : 215 + y / 14));
                raw.WriteByte((byte)(sun ? 116 : ridge ? 130 + x / 20 : 236));
            }
        }
        using var png = new MemoryStream();
        png.Write([137, 80, 78, 71, 13, 10, 26, 10]);
        byte[] header = new byte[13];
        BinaryPrimitives.WriteInt32BigEndian(header, width); BinaryPrimitives.WriteInt32BigEndian(header.AsSpan(4), height);
        header[8] = 8; header[9] = 2;
        Chunk(png, "IHDR", header);
        using var compressed = new MemoryStream();
        using (var stream = new ZLibStream(compressed, CompressionLevel.Fastest, leaveOpen: true)) stream.Write(raw.ToArray());
        Chunk(png, "IDAT", compressed.ToArray()); Chunk(png, "IEND", []);
        return png.ToArray();
    }
    private static void Chunk(Stream output, string type, byte[] bytes)
    {
        Span<byte> value = stackalloc byte[4];
        BinaryPrimitives.WriteInt32BigEndian(value, bytes.Length); output.Write(value);
        byte[] name = System.Text.Encoding.ASCII.GetBytes(type); output.Write(name); output.Write(bytes);
        uint crc = uint.MaxValue;
        foreach (byte b in name.Concat(bytes))
        {
            crc ^= b;
            for (int i = 0; i < 8; i++) crc = (crc >> 1) ^ ((crc & 1) == 0 ? 0 : 0xedb88320U);
        }
        BinaryPrimitives.WriteUInt32BigEndian(value, ~crc); output.Write(value);
    }
}
