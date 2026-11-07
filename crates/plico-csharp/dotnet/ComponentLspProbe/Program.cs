using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text;
using System.Text.Json;

if (args.Length is < 2 or > 3) throw new ArgumentException("Usage: ComponentLspProbe <csharp-ls executable> <ComponentProbe directory>");
bool interceptors = args.Length == 3 && args[2] == "--interceptors";
string root = Path.GetFullPath(args[1]), path = Path.Combine(root, "SampleViews.cs");
string uri = new Uri(path).AbsoluteUri, text = File.ReadAllText(path);
await using var lsp = new Client(args[0], root);
object Position(string marker, int offset = 0)
{
    int index = text.IndexOf(marker, StringComparison.Ordinal) + offset;
    if (index < 0) throw new InvalidOperationException("Missing probe marker");
    string before = text[..index];
    return new { line = before.Count(c => c == '\n'), character = index - (before.LastIndexOf('\n') + 1) };
}
object At(string marker, int offset = 0) => new { textDocument = new { uri }, position = Position(marker, offset) };
void Check(bool okay, string message) { if (!okay) throw new InvalidOperationException(message); }
await lsp.Request("initialize", new
{
    processId = Environment.ProcessId,
    rootUri = new Uri(root + "/").AbsoluteUri,
    workspaceFolders = new[] { new { uri = new Uri(root + "/").AbsoluteUri, name = "ComponentProbe" } },
    capabilities = new
    {
        workspace = new { configuration = true, workspaceFolders = true },
        textDocument = new { completion = new { completionItem = new { snippetSupport = true } }, definition = new { linkSupport = true } },
        experimental = new { csharp = new { metadataUris = true } }
    },
    initializationOptions = new { csharp = new { analyzersEnabled = true, useMetadataUris = true } }
});
await lsp.Notify("initialized", new { });
await lsp.Notify("textDocument/didOpen", new { textDocument = new { uri, languageId = "csharp", version = 1, text } });
Console.Error.WriteLine("Initialized LSP; waiting for generated wrapper hover");
JsonElement hover = default;
for (int attempt = 0; attempt < 60; attempt++)
{
    hover = await lsp.Request("textDocument/hover", At("SampleViews.Counter", "SampleViews.".Length + 2));
    if (hover.ValueKind != JsonValueKind.Null) break;
    await Task.Delay(500);
}
Check(hover.ValueKind is not (JsonValueKind.Null or JsonValueKind.Undefined), "Generated wrapper has no hover");
Console.Error.WriteLine("Generated wrapper resolved");
var body = await lsp.Request("textDocument/completion", At("ui.Text", 3));
var caller = await lsp.Request("textDocument/completion", At("SampleViews.Counter", "SampleViews.".Length));
string[] Labels(JsonElement result) => (result.ValueKind == JsonValueKind.Array ? result : result.GetProperty("items"))
    .EnumerateArray().Select(i => i.GetProperty("label").GetString()!).ToArray();
string[] bodyLabels = Labels(body), callerLabels = Labels(caller);
Check(bodyLabels.Any(s => s == "Text") && bodyLabels.Any(s => s == "State"), "Normal Ui completion missing inside Body");
Check(callerLabels.Any(s => s == "Counter"), "Generated Counter missing from member completion");
var definition = await lsp.Request("textDocument/definition", At("SampleViews.Counter", "SampleViews.".Length + 2));
Check(definition.ValueKind == JsonValueKind.Array && definition.GetArrayLength() > 0, "Generated wrapper has no definition target");
var first = definition[0];
string definitionUri = first.TryGetProperty("targetUri", out var target) ? target.GetString()! : first.GetProperty("uri").GetString()!;
Check(interceptors ? definitionUri == uri : definitionUri.Contains("generated", StringComparison.OrdinalIgnoreCase) || definitionUri.EndsWith(".g.cs"), "Definition target does not match the component mode");
var signature = await lsp.Request("textDocument/signatureHelp", At("SampleViews.Counter(ui,", "SampleViews.Counter(ui,".Length));
Check(signature.ValueKind != JsonValueKind.Null, "Component has no signature help");
if (interceptors) Check(!signature.ToString().Contains("__plico"), "Interceptor implementation polluted the public signature");
var rename = await lsp.Request("textDocument/rename", new { textDocument = new { uri }, position = Position("count = ref", 2), newName = "localCount" });
Check(rename.ValueKind != JsonValueKind.Null, "Local rename in Body failed");
int Offset(JsonElement point)
{
    int line = point.GetProperty("line").GetInt32(), index = 0;
    for (int i = 0; i < line; i++) index = text.IndexOf('\n', index) + 1;
    return index + point.GetProperty("character").GetInt32();
}
string renamed = text;
foreach (var edit in rename.GetProperty("changes").GetProperty(uri).EnumerateArray()
    .OrderByDescending(e => Offset(e.GetProperty("range").GetProperty("start"))))
{
    int start = Offset(edit.GetProperty("range").GetProperty("start")), end = Offset(edit.GetProperty("range").GetProperty("end"));
    renamed = renamed[..start] + edit.GetProperty("newText").GetString() + renamed[end..];
}
Check(renamed.Contains("ref int localCount = ref") && !renamed.Contains("ref int count = ref"), "Local rename edits produced the wrong source");
Console.Error.WriteLine("Completion, definition, signature help and local rename passed");
bool componentRename = false;
if (interceptors)
{
    var methodRename = await lsp.Request("textDocument/rename", new { textDocument = new { uri }, position = Position("void Counter(", 7), newName = "Renamed" });
    Check(methodRename.ValueKind != JsonValueKind.Null, "Component method rename failed");
    string source = text;
    var changes = methodRename.GetProperty("changes");
    foreach (var edit in changes.GetProperty(uri).EnumerateArray().OrderByDescending(e => Offset(e.GetProperty("range").GetProperty("start"))))
    {
        int start = Offset(edit.GetProperty("range").GetProperty("start")), end = Offset(edit.GetProperty("range").GetProperty("end"));
        source = source[..start] + edit.GetProperty("newText").GetString() + source[end..];
    }
    Check(source.Contains("void Renamed(") && source.Contains("SampleViews.Renamed(ui"), "Rename did not update declaration and calls together");
    Check(changes.EnumerateObject().All(p => !p.Name.Contains("/generated/")), "Rename requires editing generated source");
    componentRename = true;
}
// A document-only edit: check that the generator updates without writing .g.cs
// into the source tree or requiring a command-line rebuild.
string edited = interceptors ? text.Replace("void Counter(", "void Renamed(", StringComparison.Ordinal) : text.Replace("CounterBody(", "RenamedBody(", StringComparison.Ordinal);
await lsp.Notify("textDocument/didChange", new { textDocument = new { uri, version = 2 }, contentChanges = new[] { new { text = edited } } });
string[] updated = [];
for (int attempt = 0; attempt < 30; attempt++)
{
    var completion = await lsp.Request("textDocument/completion", At("SampleViews.Counter", "SampleViews.".Length));
    if (completion.ValueKind != JsonValueKind.Null) updated = Labels(completion);
    if (updated.Contains("Renamed") && !updated.Contains("Counter")) break;
    await Task.Delay(500);
}
Check(updated.Contains("Renamed") && !updated.Contains("Counter"), "Generator did not refresh after Body edit");
await lsp.Notify("textDocument/didChange", new { textDocument = new { uri, version = 3 }, contentChanges = new[] { new { text } } });
string invalid = text.Replace("Counter(ui, ref observed);", "Counter(ui, ref observed, 42);", StringComparison.Ordinal);
await lsp.Notify("textDocument/didChange", new { textDocument = new { uri, version = 4 }, contentChanges = new[] { new { text = invalid } } });
for (int attempt = 0; attempt < 30 && !lsp.HasDiagnostic(uri, "CS1503"); attempt++)
{
    _ = await lsp.Request("textDocument/hover", At("SampleViews.Counter", "SampleViews.".Length + 2));
    await Task.Delay(300);
}
Check(lsp.HasDiagnostic(uri, "CS1503"), "Generated wrapper argument type error was not published by LSP");
await lsp.Notify("textDocument/didChange", new { textDocument = new { uri, version = 5 }, contentChanges = new[] { new { text } } });
Console.WriteLine(JsonSerializer.Serialize(new
{
    result = "passed",
    mode = interceptors ? "interceptors" : "body",
    componentRename,
    typeDiagnostic = "CS1503",
    bodyCompletion = new[] { "Text", "State" },
    generatedCompletion = "Counter",
    generatedDefinition = definitionUri,
    hover,
    signature,
    localRename = true,
    liveGeneratorRefresh = true,
    limitation = interceptors ? "Only direct ordinary calls are intercepted; method-group conversions require a lambda." : "Renaming CounterBody changes the generated entry point; wrapper callers need a separate rename."
}));

sealed class Client : IAsyncDisposable
{
    private readonly Process process;
    private readonly Stream input, output;
    private readonly SemaphoreSlim writes = new(1);
    private readonly ConcurrentDictionary<int, TaskCompletionSource<JsonElement>> pending = new();
    private readonly ConcurrentDictionary<string, JsonElement> diagnostics = new();
    internal bool HasDiagnostic(string uri, string code) => diagnostics.TryGetValue(uri, out var values) && values.EnumerateArray().Any(d => d.TryGetProperty("code", out var c) && c.ToString() == code);
    private readonly Task reader;
    private int next;
    internal Client(string executable, string root)
    {
        var info = new ProcessStartInfo(Path.GetFullPath(executable))
        {
            WorkingDirectory = root,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true
        };
        info.ArgumentList.Add("--solution"); info.ArgumentList.Add(Path.GetFileName(Directory.GetFiles(root, "*.sln").Single()));
        info.ArgumentList.Add("--features"); info.ArgumentList.Add("metadata-uris");
        string runtime = Path.GetDirectoryName(typeof(object).Assembly.Location)!;
        info.Environment["DOTNET_ROOT"] = Directory.GetParent(runtime)!.Parent!.Parent!.FullName;
        process = Process.Start(info) ?? throw new InvalidOperationException("Cannot start LSP");
        _ = Task.Run(async () => { while (await process.StandardError.ReadLineAsync() is { } line) Console.Error.WriteLine("server: " + line); });
        input = process.StandardInput.BaseStream; output = process.StandardOutput.BaseStream;
        reader = Read();
    }
    private async Task Send(object message)
    {
        byte[] body = JsonSerializer.SerializeToUtf8Bytes(message);
        byte[] header = Encoding.ASCII.GetBytes($"Content-Length: {body.Length}\r\n\r\n");
        await writes.WaitAsync();
        try { await input.WriteAsync(header); await input.WriteAsync(body); await input.FlushAsync(); }
        finally { writes.Release(); }
    }
    internal Task Notify(string method, object parameters) => Send(new { jsonrpc = "2.0", method, @params = parameters });
    internal async Task<JsonElement> Request(string method, object parameters)
    {
        int id = Interlocked.Increment(ref next);
        var completion = new TaskCompletionSource<JsonElement>(TaskCreationOptions.RunContinuationsAsynchronously);
        pending[id] = completion;
        await Send(new { jsonrpc = "2.0", id, method, @params = parameters });
        return await completion.Task.WaitAsync(TimeSpan.FromSeconds(90));
    }
    private async Task Read()
    {
        try
        {
            var header = new StringBuilder(); var one = new byte[1];
            while (true)
            {
                header.Clear();
                while (!header.ToString().EndsWith("\r\n\r\n", StringComparison.Ordinal))
                {
                    if (await output.ReadAsync(one) == 0) throw new EndOfStreamException("LSP exited before response");
                    header.Append((char)one[0]); if (header.Length > 8192) throw new IOException("Invalid LSP header");
                }
                string length = header.ToString().Split("\r\n").Single(s => s.StartsWith("Content-Length:", StringComparison.OrdinalIgnoreCase))[15..].Trim();
                int count = int.Parse(length); if (count is < 0 or > 64 * 1024 * 1024) throw new IOException("Invalid LSP body length");
                byte[] body = new byte[count]; await output.ReadExactlyAsync(body);
                using var document = JsonDocument.Parse(body); var message = document.RootElement;
                if (!message.TryGetProperty("id", out var id))
                {
                    if (message.TryGetProperty("method", out var notification) && notification.GetString() == "textDocument/publishDiagnostics")
                    {
                        var parameters = message.GetProperty("params");
                        diagnostics[parameters.GetProperty("uri").GetString()!] = parameters.GetProperty("diagnostics").Clone();
                    }
                    continue;
                }
                if (message.TryGetProperty("method", out var method))
                {
                    object? result = null;
                    if (method.GetString() == "workspace/configuration") result = message.GetProperty("params").GetProperty("items").EnumerateArray()
                        .Select(_ => new { analyzersEnabled = true, useMetadataUris = true }).ToArray();
                    await Send(new { jsonrpc = "2.0", id = id.Clone(), result });
                }
                else if (pending.TryRemove(id.GetInt32(), out var completion))
                {
                    if (message.TryGetProperty("error", out var error)) completion.SetException(new InvalidOperationException(error.ToString()));
                    else completion.SetResult(message.GetProperty("result").Clone());
                }
            }
        }
        catch (Exception error) { foreach (var item in pending.Values) item.TrySetException(error); }
    }
    public async ValueTask DisposeAsync()
    {
        try { if (!process.HasExited && !reader.IsCompleted) { await Request("shutdown", new { }).WaitAsync(TimeSpan.FromSeconds(5)); await Notify("exit", new { }); } }
        catch (Exception) { }
        if (!process.HasExited) process.Kill(true);
        await reader; process.Dispose(); writes.Dispose();
    }
}
