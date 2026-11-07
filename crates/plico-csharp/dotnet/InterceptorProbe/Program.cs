using Plico;
using Plico.InterceptorProbe;

if (args.Contains("--self-test")) { RuntimeChecks.Run(); return; }
int observed = 0;
using var app = new Application(ui => SampleViews.Counter(ui, ref observed));
if (args.Contains("--watch-probe"))
{
    app.Step(); app.Click("button", "Increment Counter"); app.Step();
    var elapsed = System.Diagnostics.Stopwatch.StartNew(); string previous = "";
    while (elapsed.Elapsed.TotalSeconds < 120)
    {
        app.Step();
        string current = $"pid={Environment.ProcessId} count={observed} caption={SampleViews.Version}";
        if (current != previous) { Console.WriteLine(current); previous = current; }
        Thread.Sleep(30);
    }
}
else if (args.Length == 2 && args[0] == "--capture") app.Capture(args[1]);
else app.Run("Interceptor component probe", 640, 480);
