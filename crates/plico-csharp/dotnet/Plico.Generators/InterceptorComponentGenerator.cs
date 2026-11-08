using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Text;

namespace Plico.Generators;

[Generator(LanguageNames.CSharp)]
public sealed class InterceptorComponentGenerator : IIncrementalGenerator
{
    private static readonly DiagnosticDescriptor Unsupported = new("PLICO010", "Unsupported intercepted component",
        "Cannot intercept component '{0}': {1}", "Plico", DiagnosticSeverity.Error, true);
    private static readonly DiagnosticDescriptor Delegate = new("PLICO012", "Component method group bypasses identity",
        "Method-group use of component '{0}' bypasses identity injection; use an explicit lambda that invokes it", "Plico", DiagnosticSeverity.Warning, true);
    private static readonly SymbolDisplayFormat Format = SymbolDisplayFormat.FullyQualifiedFormat.WithMiscellaneousOptions(
        SymbolDisplayFormat.FullyQualifiedFormat.MiscellaneousOptions | SymbolDisplayMiscellaneousOptions.IncludeNullableReferenceTypeModifier);
    private sealed class Call
    {
        internal readonly InvocationExpressionSyntax Syntax;
        internal readonly SemanticModel Model;
        internal readonly IMethodSymbol Method;
        internal Call(InvocationExpressionSyntax syntax, SemanticModel model, IMethodSymbol method) { Syntax = syntax; Model = model; Method = method; }
    }
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var enabled = context.AnalyzerConfigOptionsProvider.Select(static (o, _) =>
            o.GlobalOptions.TryGetValue("build_property.PlicoComponentInterceptors", out var value) && string.Equals(value, "true", StringComparison.OrdinalIgnoreCase));
        var calls = context.SyntaxProvider.CreateSyntaxProvider(
            static (node, _) => node is InvocationExpressionSyntax && !node.SyntaxTree.FilePath.EndsWith(".g.cs", StringComparison.Ordinal),
            static (ctx, ct) => {
                var syntax = (InvocationExpressionSyntax)ctx.Node;
                var symbol = ctx.SemanticModel.GetSymbolInfo(syntax, ct);
                var method = symbol.Symbol as IMethodSymbol ?? symbol.CandidateSymbols.OfType<IMethodSymbol>().FirstOrDefault();
                return method is not null && Component(method) ? new Call(syntax, ctx.SemanticModel, method) : null;
            }).Where(static call => call is not null);
        context.RegisterSourceOutput(calls.Collect().Combine(enabled), static (ctx, pair) => {
            if (!pair.Right) return;
            var bridges = new HashSet<string>(StringComparer.Ordinal);
            var ordinals = new Dictionary<string, int>(StringComparer.Ordinal);
            foreach (var candidate in pair.Left.OrderBy(c => c!.Syntax.SyntaxTree.FilePath, StringComparer.Ordinal).ThenBy(c => c!.Syntax.SpanStart))
            {
                var call = candidate!;
                ISymbol? caller = call.Model.GetEnclosingSymbol(call.Syntax.SpanStart, ctx.CancellationToken);
                while (caller is IMethodSymbol m && (m.MethodKind == MethodKind.AnonymousFunction || m.MethodKind == MethodKind.LocalFunction)) caller = caller.ContainingSymbol;
                string key = call.Syntax.SyntaxTree.FilePath + "|" + caller?.ToDisplayString(SymbolDisplayFormat.CSharpErrorMessageFormat);
                ordinals.TryGetValue(key, out int ordinal); ordinals[key] = ordinal + 1;
                // Keep implementation symbols stable when text before a call changes length.
                Generate(ctx, call, bridges, Hash(key + "|" + ordinal.ToString(CultureInfo.InvariantCulture)));
            }
        });
        var groups = context.SyntaxProvider.CreateSyntaxProvider(
            static (node, _) => node is IdentifierNameSyntax || node is MemberAccessExpressionSyntax,
            static (ctx, ct) => {
                var node = ctx.Node;
                if (node is IdentifierNameSyntax name && (name.Parent is MemberAccessExpressionSyntax access && access.Name == name
                    || name.Parent is MemberBindingExpressionSyntax binding && binding.Name == name)) return null;
                if (node.Parent is InvocationExpressionSyntax invocation && invocation.Expression == node) return null;
                if (node.Ancestors().OfType<InvocationExpressionSyntax>().Any(i => i.Expression is IdentifierNameSyntax id && id.Identifier.ValueText == "nameof")) return null;
                var info = ctx.SemanticModel.GetSymbolInfo(node, ct);
                var method = info.Symbol as IMethodSymbol ?? info.CandidateSymbols.OfType<IMethodSymbol>().FirstOrDefault();
                return method is not null && Component(method) ? Diagnostic.Create(Delegate, node.GetLocation(), method.Name) : null;
            }).Where(static diagnostic => diagnostic is not null);
        context.RegisterSourceOutput(groups.Combine(enabled), static (ctx, pair) => { if (pair.Right) ctx.ReportDiagnostic(pair.Left!); });
    }
    private static bool Component(IMethodSymbol method) => method.GetAttributes().Any(a => a.AttributeClass?.ToDisplayString() == "Plico.ComponentAttribute");
    private static string? Invalid(IMethodSymbol method)
    {
        var type = method.ContainingType;
        if (method.MethodKind != MethodKind.Ordinary || !method.ReturnsVoid || method.IsAsync || method.IsGenericMethod || method.IsExtensionMethod
            || method.IsAbstract || method.IsVirtual || method.IsOverride || method.IsExtern)
            return "use an ordinary synchronous non-virtual, non-generic void method";
        if (type.TypeKind != TypeKind.Class || type.IsGenericType || type.ContainingType is not null || type.IsRecord || type.IsFileLocal)
            return "use a top-level non-generic class";
        if (method.Parameters.Length == 0 || method.Parameters[0].Type.ToDisplayString() != "Plico.Ui" || method.Parameters[0].RefKind != RefKind.None)
            return "the first parameter must be Plico.Ui passed by value";
        if (method.Parameters.Any(p => p.Name.StartsWith("__plico", StringComparison.Ordinal) || p.Type.TypeKind == TypeKind.Pointer || p.Type.TypeKind == TypeKind.FunctionPointer))
            return "reserved __plico and pointer parameters are unsupported";
        return null;
    }
    private static void Generate(SourceProductionContext context, Call call, HashSet<string> bridges, string site)
    {
        var method = call.Method; var type = method.ContainingType;
        string? error = Invalid(method);
        bool bridge = method.DeclaredAccessibility != Accessibility.Public && method.DeclaredAccessibility != Accessibility.Internal
            && method.DeclaredAccessibility != Accessibility.ProtectedOrInternal;
        if (error is null && bridge && (type.DeclaringSyntaxReferences.Length == 0 || type.DeclaringSyntaxReferences.Any(r =>
            r.GetSyntax(context.CancellationToken) is not ClassDeclarationSyntax c || !c.Modifiers.Any(SyntaxKind.PartialKeyword))))
            error = "private/protected methods require a partial class for an access bridge";
        if (error is not null) { context.ReportDiagnostic(Diagnostic.Create(Unsupported, call.Syntax.GetLocation(), method.Name, error)); return; }
        var location = call.Model.GetInterceptableLocation(call.Syntax, context.CancellationToken);
        if (location is null) { context.ReportDiagnostic(Diagnostic.Create(Unsupported, call.Syntax.GetLocation(), method.Name, "this invocation cannot be intercepted")); return; }
        string identity = method.ContainingAssembly.Identity.Name + "|" + method.ToDisplayString(SymbolDisplayFormat.CSharpErrorMessageFormat);
        string component = Hash(identity);
        string owner = type.ToDisplayString(Format);
        string receiver = method.IsStatic ? "" : "this " + owner + " __plicoReceiver, ";
        string parameters = string.Join(", ", method.Parameters.Select(Parameter));
        string arguments = string.Join(", ", method.Parameters.Select(p => ArgumentRef(p.RefKind) + "@" + p.Name));
        var span = call.Syntax.GetLocation().GetMappedLineSpan();
        var source = new StringBuilder("// <auto-generated/>\n#nullable enable\n");
        source.Append("namespace System.Runtime.CompilerServices {\n[global::System.AttributeUsage(global::System.AttributeTargets.Method, AllowMultiple=true)]\n")
            .Append("file sealed class InterceptsLocationAttribute : global::System.Attribute { public InterceptsLocationAttribute(int version, string data) { } }\n}\n");
        source.Append("namespace Plico.Generated {\nfile static class ComponentInterceptors {\n")
            .Append(location.GetInterceptsLocationAttributeSyntax()).Append("\npublic static void Call_").Append(site).Append('(').Append(receiver).Append(parameters).Append(") {\n")
            .Append("using var __plicoSite = @").Append(method.Parameters[0].Name).Append(".Scope(file: ").Append(SymbolDisplay.FormatLiteral(span.Path, true))
            .Append(", line: ").Append(span.StartLinePosition.Line + 1).Append(");\n")
            .Append("using var __plicoType = @").Append(method.Parameters[0].Name).Append(".Scope(0x").Append(component).Append("UL);\n");
        if (bridge) source.Append(owner).Append(".__PlicoOriginal_").Append(component).Append('(').Append(method.IsStatic ? "" : "__plicoReceiver, ").Append(arguments);
        else source.Append(method.IsStatic ? owner : "__plicoReceiver").Append(".@").Append(method.Name).Append('(').Append(arguments);
        source.Append(");\n}\n}\n}\n");
        context.AddSource("Plico.Interceptor." + site + ".g.cs", SourceText.From(source.ToString(), Encoding.UTF8));
        if (bridge && bridges.Add(component))
        {
            var body = new StringBuilder("// <auto-generated/>\n#nullable enable\n");
            bool ns = !type.ContainingNamespace.IsGlobalNamespace;
            if (ns) body.Append("namespace ").Append(type.ContainingNamespace.ToDisplayString()).Append(" {\n");
            body.Append(type.DeclaredAccessibility == Accessibility.Public ? "public " : "internal ").Append(type.IsStatic ? "static " : "")
                .Append("partial class @").Append(type.Name).Append(" {\ninternal static void __PlicoOriginal_").Append(component).Append('(')
                .Append(method.IsStatic ? "" : owner + " __plicoReceiver, ").Append(parameters).Append(") { ")
                .Append(method.IsStatic ? "" : "__plicoReceiver.").Append('@').Append(method.Name).Append('(').Append(arguments).Append("); }\n}\n");
            if (ns) body.Append("}\n");
            context.AddSource("Plico.Bridge." + component + ".g.cs", SourceText.From(body.ToString(), Encoding.UTF8));
        }
    }
    private static string Parameter(IParameterSymbol p) => (p.ScopedKind != ScopedKind.None ? "scoped " : "") +
        (p.RefKind == RefKind.RefReadOnlyParameter ? "ref readonly " : ArgumentRef(p.RefKind)) + p.Type.ToDisplayString(Format) + " @" + p.Name;
    private static string ArgumentRef(RefKind kind) => kind == RefKind.Ref ? "ref " : kind == RefKind.Out ? "out " : kind == RefKind.In || kind == RefKind.RefReadOnlyParameter ? "in " : "";
    private static string Hash(string text)
    {
        ulong hash = 14695981039346656037UL;
        foreach (char c in text) hash = unchecked((hash ^ c) * 1099511628211UL);
        return hash.ToString("x16", CultureInfo.InvariantCulture);
    }
}
