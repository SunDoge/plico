using Plico;
namespace Plico.InterceptorProbe;

internal static partial class DelegateChecks
{
    [Component]
    private static void Local(Ui ui)
    {
        ref int value = ref ui.State(static () => 0, key: "delegate-slot"); _ = value;
    }
    internal static void Run()
    {
        // Intentionally bypass the diagnostic to demonstrate what the compiler
        // cannot intercept. Production code should use the lambda form below.
#pragma warning disable PLICO012
        Action<Ui> unwrapped = Local;
#pragma warning restore PLICO012
        using (var app = new Application(ui => { unwrapped(ui); unwrapped(ui); }))
        {
            bool rejected = false;
            try { app.Step(); } catch (InvalidOperationException) { rejected = true; }
            if (!rejected) throw new InvalidOperationException("Delegate calls were unexpectedly scoped");
        }
        using (var app = new Application(ui => { Local(ui); Local(ui); })) app.Step();
        Console.WriteLine("Delegate boundary verified: method groups bypass interception; explicit lambda calls receive scopes");
    }
}
