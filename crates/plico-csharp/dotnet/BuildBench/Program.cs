// Run from the repository root. Both source probes are restored in finally.
using System.Diagnostics;
using System.Text.Json;

string root = Path.GetFullPath(args.Length == 0 ? "." : args[0]);
string cs = Path.Combine(root, "crates/plico-csharp/dotnet/Demo/Program.cs");
string rust = Path.Combine(root, "crates/plico-csharp/examples/native.rs");
string csOriginal = File.ReadAllText(cs), rustOriginal = File.ReadAllText(rust);
var csBuild = new[] { "build", "crates/plico-csharp/dotnet/Demo", "--no-restore", "-v", "quiet" };
var rustBuild = new[] { "build", "--locked", "-p", "plico-csharp", "--example", "native" };
var csTimes = new List<double>(); var rustTimes = new List<double>();
var csReady = new List<double>(); var rustReady = new List<double>();
string output = Path.Combine(root, "reports/csharp"); Directory.CreateDirectory(output);
Run("dotnet", csBuild); Run("cargo", rustBuild);
double csNoop = Run("dotnet", csBuild), rustNoop = Run("cargo", rustBuild);
try
{
    for (int i = 0; i < 3; i++)
    {
        string nonce = Stopwatch.GetTimestamp().ToString();
        File.WriteAllText(cs, csOriginal.Replace("Plico, from C#", $"Plico, C# probe {nonce}"));
        csTimes.Add(Run("dotnet", csBuild));
        File.WriteAllText(rust, rustOriginal.Replace("Plico, from Rust", $"Plico, Rust probe {nonce}"));
        rustTimes.Add(Run("cargo", rustBuild));
    }
}
finally
{
    File.WriteAllText(cs, csOriginal); File.WriteAllText(rust, rustOriginal);
    Run("dotnet", csBuild); Run("cargo", rustBuild);
}
// Restart/capture is measured separately; GPU startup must not be mistaken for compilation.
for (int i = 0; i < 3; i++)
{
    csReady.Add(Run("dotnet", ["crates/plico-csharp/dotnet/Demo/bin/Debug/net10.0/Demo.dll", "--capture", "reports/csharp/csharp-ready.png"]));
    rustReady.Add(Run("target/debug/examples/native", ["--capture", "reports/csharp/rust-ready.png"]));
}
var result = new
{
    csharpEditSeconds = csTimes,
    rustEditSeconds = rustTimes,
    csharpNoopSeconds = csNoop,
    rustNoopSeconds = rustNoop,
    csharpRestartCaptureSeconds = csReady,
    rustRestartCaptureSeconds = rustReady,
    note = "Warm toolchains; .NET Debug vs Rust workspace dev opt=1/deps opt=2. GPU restart measured separately. No cold-build or large-app conclusion."
};
string json = JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true });
File.WriteAllText(Path.Combine(output, "build-feedback.json"), json); Console.WriteLine(json);

double Run(string program, string[] arguments)
{
    var start = new ProcessStartInfo(program) { WorkingDirectory = root, RedirectStandardOutput = true, RedirectStandardError = true };
    foreach (string argument in arguments) start.ArgumentList.Add(argument);
    var clock = Stopwatch.StartNew(); using var process = Process.Start(start)!;
    var stdout = process.StandardOutput.ReadToEndAsync(); var stderr = process.StandardError.ReadToEndAsync();
    process.WaitForExit(); Task.WaitAll(stdout, stderr); clock.Stop();
    if (process.ExitCode != 0) throw new InvalidOperationException($"{program} failed: {stdout.Result}\n{stderr.Result}");
    return clock.Elapsed.TotalSeconds;
}
