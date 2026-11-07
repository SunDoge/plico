using Plico;
using Plico.ComponentProbe;

int observed = 0;
using var app = new Application(ui => SampleViews.Counter(ui, ref observed));
if (args.Contains("--watch-probe"))
{
    app.Step(); app.Click("button", "Increment"); app.Step();
    var elapsed = System.Diagnostics.Stopwatch.StartNew();
    string previous = "";
    while (elapsed.Elapsed.TotalSeconds < 60)
    {
        app.Step();
        string current = $"pid={Environment.ProcessId} count={observed} caption={SampleViews.Version}";
        if (current != previous) { Console.WriteLine(current); previous = current; }
        Thread.Sleep(30);
    }
}
else app.Run("Generated component probe", 640, 480);
