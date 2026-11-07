using System.Runtime.InteropServices;
using Plico.Interop;
namespace Plico;

/// Explicit native prerequisite. Checking/loading never invokes Cargo.
public static class NativeRuntime
{
    public const uint AbiVersion = 5;
    public const uint ProtocolRevision = 4;
    public static string? LibraryPath { get; private set; }
    private static readonly object loadGate = new();
    private static IntPtr libraryHandle;
    private const string BuildHelp = "Build the native foundation explicitly with: cargo build --locked -p plico-csharp --release. Then restart the app. Ordinary C# edits do not require Cargo.";

    static NativeRuntime() => NativeLibrary.SetDllImportResolver(typeof(Application).Assembly, (name, assembly, searchPath) =>
    {
        if (name != "plico_csharp") return IntPtr.Zero;
        lock (loadGate)
        {
            if (libraryHandle != IntPtr.Zero) return libraryHandle;
            string? path = FindLibrary();
            if (path is null) return IntPtr.Zero; // Let .NET search deployed/system native libraries.
            LibraryPath = path;
            return libraryHandle = NativeLibrary.Load(path);
        }
    });

    internal static void Initialize() { } // Forces resolver installation without loading a library.

    /// Loads and verifies the library before allocating a host or invoking UI code.
    public static unsafe void CheckCompatibility()
    {
        try
        {
            // Evaluate ABI first: never call a newer export through an old ABI.
            uint abi = Native.plico_abi_version();
            if (abi != AbiVersion) Validate(abi, 0, 0, sizeof(Command));
            uint size = Native.plico_command_size();
            if (size != sizeof(Command)) Validate(abi, size, 0, sizeof(Command));
            Validate(abi, size, Native.plico_protocol_revision(), sizeof(Command));
        }
        catch (DllNotFoundException error)
        { throw new InvalidOperationException($"Plico native library was not found or one of its system dependencies could not load. {Describe()} {BuildHelp}", error); }
        catch (BadImageFormatException error)
        { throw new InvalidOperationException($"Plico native library has the wrong format/architecture for {RuntimeInformation.RuntimeIdentifier}. {Describe()} {BuildHelp}", error); }
        catch (EntryPointNotFoundException error)
        { throw new InvalidOperationException($"Plico native library is too old or is missing a required export. Expected ABI {AbiVersion}, protocol {ProtocolRevision}. {Describe()} {BuildHelp}", error); }
    }
    internal static unsafe string LastError()
    {
        uint count = Native.plico_last_error(null, 0);
        byte[] bytes = new byte[checked((int)count)];
        fixed (byte* pointer = bytes) Native.plico_last_error(pointer, count);
        return System.Text.Encoding.UTF8.GetString(bytes);
    }
    internal static void Validate(uint abi, uint size, uint protocol, int expectedSize)
    {
        if (abi != AbiVersion || size != expectedSize || protocol != ProtocolRevision)
            throw new InvalidOperationException($"Plico native/managed mismatch: expected ABI {AbiVersion}, command size {expectedSize}, protocol {ProtocolRevision}; found ABI {abi}, command size {size}, protocol {protocol}. {Describe()} {BuildHelp}");
    }
    private static string Describe() => "Library: " + (LibraryPath ?? Environment.GetEnvironmentVariable("PLICO_NATIVE_LIBRARY") ?? "default .NET native search") + ".";
    private static string? FindLibrary()
    {
        string? explicitPath = Environment.GetEnvironmentVariable("PLICO_NATIVE_LIBRARY");
        if (!string.IsNullOrWhiteSpace(explicitPath)) return Path.GetFullPath(explicitPath);
        string file = OperatingSystem.IsWindows() ? "plico_csharp.dll" : OperatingSystem.IsMacOS() ? "libplico_csharp.dylib" : "libplico_csharp.so";
        foreach (string path in new[] { Path.Combine(AppContext.BaseDirectory, file), Path.Combine(AppContext.BaseDirectory, "runtimes", RuntimeInformation.RuntimeIdentifier, "native", file) })
            if (File.Exists(path)) return path;
        // Source-checkout convenience only: never run a compiler, download files,
        // or silently substitute another library for an explicit user path.
        foreach (string start in new[] { Environment.CurrentDirectory, AppContext.BaseDirectory })
            for (DirectoryInfo? dir = new(start); dir is not null; dir = dir.Parent)
                if (File.Exists(Path.Combine(dir.FullName, "crates", "plico-csharp", "Cargo.toml")))
                {
                    string? configured = Environment.GetEnvironmentVariable("CARGO_TARGET_DIR");
                    string target = string.IsNullOrEmpty(configured) ? Path.Combine(dir.FullName, "target") : Path.GetFullPath(configured, dir.FullName);
                    var candidates = new[] { Path.Combine(target, "release", file), Path.Combine(target, "debug", file) };
                    return candidates.Where(File.Exists).OrderByDescending(File.GetLastWriteTimeUtc).FirstOrDefault();
                }
        return null;
    }
}
