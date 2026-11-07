using System.Numerics;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text;
using Plico.Interop;
namespace Plico;

public enum WgslMode { Effect, Module, Backdrop }

/// Immutable authored WGSL. Creating/loading a program does not load Rust.
/// Validation and GPU pipeline creation happen at runtime, never through rustc.
public sealed record WgslProgram
{
    public string Source { get; }
    public string Name { get; }
    public WgslMode Mode { get; }
    public WgslProgram(string source, string name = "inline.wgsl", WgslMode mode = WgslMode.Effect)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(source); ArgumentException.ThrowIfNullOrWhiteSpace(name);
        if (Encoding.UTF8.GetByteCount(source) > 65536) throw new ArgumentException("WGSL source exceeds 64 KiB", nameof(source));
        if (!Enum.IsDefined(mode)) throw new ArgumentOutOfRangeException(nameof(mode));
        Source = source; Name = name; Mode = mode;
    }
    /// Load once at startup, outside the UI declaration. Use WgslFile for live files.
    public static WgslProgram Load(string path, WgslMode mode = WgslMode.Effect) =>
        new(Read(path), Path.GetFullPath(path), mode);
    public static WgslProgram Embedded(Assembly assembly, string resource, WgslMode mode = WgslMode.Effect)
    {
        ArgumentNullException.ThrowIfNull(assembly);
        using var stream = assembly.GetManifestResourceStream(resource) ?? throw new FileNotFoundException($"Missing WGSL resource {resource}");
        return new(Read(stream), resource, mode);
    }
    internal static string Read(string path)
    {
        // A bounded stream also covers files that grow between stat and read.
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        return Read(stream);
    }
    private static string Read(Stream stream)
    {
        byte[] bytes = new byte[65537]; int count = 0, read;
        while (count < bytes.Length && (read = stream.Read(bytes, count, bytes.Length - count)) > 0) count += read;
        if (count > 65536) throw new InvalidDataException("WGSL source exceeds 64 KiB");
        return new UTF8Encoding(false, true).GetString(bytes, 0, count).TrimStart('\uFEFF');
    }
    /// Naga validation only: no GPU/window. Returns a diagnostic, or null on success.
    public unsafe string? Validate()
    {
        NativeRuntime.CheckCompatibility();
        byte[] bytes = Encoding.UTF8.GetBytes(Source);
        uint flags = Mode == WgslMode.Module ? 1U : Mode == WgslMode.Backdrop ? 2U : 0U;
        fixed (byte* pointer = bytes)
            return Native.plico_validate_shader(pointer, (uint)bytes.Length, flags) == 1 ? null : $"{Name}: {NativeRuntime.LastError()}";
    }
}

public sealed unsafe partial class Ui
{
    public Builder Shader(WgslProgram program, bool animated = false, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        ArgumentNullException.ThrowIfNull(program);
        var builder = Shader(program.Source, animated, key, file, line);
        if (program.Mode == WgslMode.Module) builder.ShaderModule();
        if (program.Mode == WgslMode.Backdrop) builder.ReadBackdrop();
        return builder;
    }
    public readonly partial struct Builder
    {
        /// One vec4 slot in plico.values[0..63]. Scalars use x and clear yzw.
        public Builder Uniform(int slot, float value) => Uniform(slot, new Vector4(value, 0, 0, 0));
        public Builder Uniform(int slot, Vector2 value) => Uniform(slot, new Vector4(value, 0, 0));
        public Builder Uniform(int slot, Vector3 value) => Uniform(slot, new Vector4(value, 0));
        public Builder Uniform(int slot, Vector4 value)
        {
            _ = Current;
            if ((uint)slot >= 64 || !float.IsFinite(value.X) || !float.IsFinite(value.Y) || !float.IsFinite(value.Z) || !float.IsFinite(value.W))
                throw new ArgumentOutOfRangeException(nameof(slot));
            var properties = ui.Properties(index).Shader ??= new();
            if (properties.Module) throw new InvalidOperationException("Uniform uses the effect prelude. Complete modules must use ShaderValues with their own buffer layout.");
            int count = (slot + 1) * 4;
            var values = properties.Values;
            if (values is null || values.Length < count) Array.Resize(ref values, count);
            values![slot * 4] = value.X; values[slot * 4 + 1] = value.Y;
            values[slot * 4 + 2] = value.Z; values[slot * 4 + 3] = value.W;
            properties.Values = values;
            return this;
        }
    }
}
