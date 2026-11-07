using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Plico;
using Plico.Generators;

var refs = ((string)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES")!).Split(Path.PathSeparator)
    .Select(p => MetadataReference.CreateFromFile(p)).ToList();
refs.Add(MetadataReference.CreateFromFile(typeof(Ui).Assembly.Location));
var parse = new CSharpParseOptions(LanguageVersion.Preview);
(int Errors, string Text, string[] Ids) Compile(string source)
{
    var compilation = CSharpCompilation.Create("ComponentCases", [CSharpSyntaxTree.ParseText(source, parse)], refs,
        new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary, nullableContextOptions: NullableContextOptions.Enable));
    GeneratorDriver driver = CSharpGeneratorDriver.Create([new ComponentGenerator().AsSourceGenerator()], parseOptions: parse);
    driver = driver.RunGeneratorsAndUpdateCompilation(compilation, out var output, out var generatorDiagnostics);
    var errors = output.GetDiagnostics().Concat(generatorDiagnostics).Where(d => d.Severity == DiagnosticSeverity.Error).ToArray();
    return (errors.Length, string.Join("\n", driver.GetRunResult().GeneratedTrees.Select(t => t.ToString())), errors.Select(d => d.Id).ToArray());
}
void Good(string members)
{
    var result = Compile("using Plico; namespace Example; public partial class Views { " + members + " }");
    if (result.Errors != 0) throw new InvalidOperationException("Valid component failed: " + string.Join(",", result.Ids) + "\n" + result.Text);
    if (!result.Text.Contains("using var __plicoScope") || !result.Text.Contains("CallerLineNumberAttribute")) throw new InvalidOperationException("Generated identity wrapper missing");
}
Good("[Component] public static void CounterBody(Ui ui) {} public void Show(Ui ui) { Counter(ui); Counter(ui); }");
Good("[Component] private void ItemBody(Ui cx, ref int count, string? label = null) { count++; } public void Show(Ui ui) { int x = 0; Item(ui, ref x); }");
Good("[Component] public static void ItemBody(Ui ui, int initial = 2, string label = \"a\\\"b\", char marker = '\\n', double value = double.NaN) {} public void Show(Ui ui) { Item(ui); }");
Good("[Component] static void ItemBody(Ui ui, out int value) { value = 1; } void Show(Ui ui) { Item(ui, out var value); }");
Good("[Component] static void ItemBody(Ui ui, in int value) {} void Show(Ui ui) { int value=0; Item(ui, in value); }");
Good("[Component] static void ItemBody(Ui ui, int value) {} [Component] static void ItemBody(Ui ui, string value) {} void Show(Ui ui) { Item(ui, 1); Item(ui, \"x\"); }");
void Bad(string source, string expected)
{
    var result = Compile("using Plico; " + source);
    if (!result.Ids.Contains(expected)) throw new InvalidOperationException($"Expected {expected}, got {string.Join(',', result.Ids)}");
}
Bad("partial class Views { [Component] static void Counter(Ui ui) {} }", "PLICO001");
Bad("class Views { [Component] static void CounterBody(Ui ui) {} }", "PLICO001");
Bad("partial class Views { [Component] static int CounterBody(Ui ui) => 1; }", "PLICO001");
Bad("partial class Views { [Component] static void CounterBody(int value) {} }", "PLICO001");
Bad("partial class Views { [Component] static void CounterBody<T>(Ui ui) {} }", "PLICO001");
Bad("partial class Views { [Component] static void CounterBody(Ui ui, params int[] values) {} }", "PLICO001");
Bad("partial class Views { [Component] static void CounterBody(Ui ui) {} static void Counter(Ui ui) {} }", "PLICO002");
Console.WriteLine("Component generator checks passed: static/instance, nullable/optional/ref/out/in, overloads and diagnostics");

void Intercept(string members, string? expected = null)
{
    var options = new CSharpParseOptions(LanguageVersion.CSharp13).WithFeatures([
        new KeyValuePair<string,string>("InterceptorsNamespaces", "Plico.Generated")]);
    var compilation = CSharpCompilation.Create("InterceptorCases", [CSharpSyntaxTree.ParseText("using Plico; " + members, options, "/_/InterceptorCase.cs")], refs,
        new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary, nullableContextOptions: NullableContextOptions.Enable, generalDiagnosticOption: ReportDiagnostic.Error));
    GeneratorDriver driver = CSharpGeneratorDriver.Create([new ComponentGenerator().AsSourceGenerator(), new InterceptorComponentGenerator().AsSourceGenerator()],
        parseOptions: options, optionsProvider: new InterceptorOptions());
    driver = driver.RunGeneratorsAndUpdateCompilation(compilation, out var output, out var diagnostics);
    var all = output.GetDiagnostics().Concat(diagnostics).ToArray();
    if (expected is not null)
    {
        if (!all.Any(d => d.Id == expected)) throw new InvalidOperationException($"Expected {expected}: " + string.Join("\n", all.Select(d => d.ToString())));
    }
    else
    {
        var errors = all.Where(d => d.Severity == DiagnosticSeverity.Error).ToArray();
        if (errors.Length > 0) throw new InvalidOperationException(string.Join("\n", errors.Select(d => d.ToString())));
        if (!driver.GetRunResult().GeneratedTrees.Any(t => t.ToString().Contains("InterceptsLocationAttribute"))) throw new InvalidOperationException("No interceptor generated");
    }
}
Intercept("public static class Views { [Component] public static void Counter(Ui ui) {} public static void Show(Ui ui) { Counter(ui); Counter(ui); } }");
Intercept("public partial class Views { [Component] private void Counter(Ui ui, ref int n, string? label = null) { n++; } public void Show(Ui ui) { int n=0; Counter(ui, ref n); } }");
Intercept("public static class Views { [Component] public static void Counter(Ui ui, out int n, params int[] values) { n=values.Length; } public static void Show(Ui ui) { Counter(ui, out int n, 1, 2); } }");
Intercept("public static class Views { [Component] public static void Counter(Ui ui, in int n) {} public static void Show(Ui ui) { int n=0; Counter(ui, in n); } }");
Intercept("public class Views { [Component] public void Counter(Ui ui) {} public void Show(Ui ui) { Counter(ui); } }");
Intercept("public class Views { [Component] public void Counter(Ui ui) {} public static void Show(Views? view, Ui ui) { view?.Counter(ui); } }");
Intercept("public class Views { [Component] private void Counter(Ui ui) {} public void Show(Ui ui) { Counter(ui); } }", "PLICO010");
Intercept("public static class Views { [Component] public static void Counter(Ui ui) {} public static System.Action<Ui> Group() => Counter; }", "PLICO012");
Intercept("public static class Views { [Component] public static void Counter(Ui ui) {} public static string Name() => nameof(Counter); public static void Show(Ui ui) { Counter(ui); } }");
Console.WriteLine("Interceptor compiler checks passed: static/instance/private, nullable/ref/out/in/params, stable language version and delegate diagnostics");

sealed class InterceptorOptions : Microsoft.CodeAnalysis.Diagnostics.AnalyzerConfigOptionsProvider
{
    private sealed class Values : Microsoft.CodeAnalysis.Diagnostics.AnalyzerConfigOptions
    {
        public override bool TryGetValue(string key, out string value) { value = "true"; return key == "build_property.PlicoComponentInterceptors"; }
    }
    private readonly Values values = new();
    public override Microsoft.CodeAnalysis.Diagnostics.AnalyzerConfigOptions GlobalOptions => values;
    public override Microsoft.CodeAnalysis.Diagnostics.AnalyzerConfigOptions GetOptions(SyntaxTree tree) => values;
    public override Microsoft.CodeAnalysis.Diagnostics.AnalyzerConfigOptions GetOptions(AdditionalText text) => values;
}
