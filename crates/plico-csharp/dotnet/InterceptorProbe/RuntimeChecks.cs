using Plico;
namespace Plico.InterceptorProbe;

internal static partial class PrivateViews
{
    internal static int Value;
    [Component]
    private static void Counter(Ui ui, int initial = 7)
    {
        ref int value = ref ui.State(() => initial, key: "private-value");
        if (ui.Button("Private increment").Clicked()) value++;
        Value = value;
    }
    internal static void Show(Ui ui) => Counter(ui);
}
internal sealed partial class InstanceView
{
    internal int Value;
    [Component]
    private void Counter(Ui ui)
    {
        ref int value = ref ui.State(static () => 3);
        if (ui.Button("Instance increment").Clicked()) value++;
        Value = value;
    }
    internal void NestedShow(Ui ui) => Counter(ui);
}
internal static class KindViews
{
    internal static int Value;
    [Component] internal static void First(Ui ui) { ref int value = ref ui.State(static () => 3, key: "shared"); Value = value; }
    [Component] internal static void Second(Ui ui) { ref int value = ref ui.State(static () => 99, key: "shared"); Value = value; }
}
internal static class NestedViews
{
    [Component]
    internal static void Parent(Ui ui, ref int left, ref int right, string prefix)
    {
        SampleViews.Counter(ui, ref left, prefix + " left");
        SampleViews.Counter(ui, ref right, prefix + " right");
    }
}
internal static class RuntimeChecks
{
    private static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
    internal static void Run()
    {
        int first = 0, second = 0; bool mounted = true;
        using (var app = new Application(ui =>
        {
            if (mounted) { SampleViews.Counter(ui, ref first, "First"); SampleViews.Counter(ui, ref second, "Second"); }
        }))
        {
            app.Step(); app.Click("button", "Increment First"); app.Step();
            Check(first == 1 && second == 0, "Same-line component calls shared state");
            app.Click("button", "Increment Second"); app.Step(); Check(first == 1 && second == 1, "Ref forwarding or event settling failed");
            HotReload.UpdateApplication(null); app.Step(); Check(first == 1 && second == 1, "Invalidation reset component state");
            mounted = false; app.Invalidate(); app.Step(); mounted = true; app.Invalidate(); app.Step();
            Check(first == 0 && second == 0, "Remount did not reset state");
        }
        using (var app = new Application(ui => PrivateViews.Show(ui)))
        {
            app.Step(); app.Click("button", "Private increment"); app.Step(); Check(PrivateViews.Value == 8, "Private static bridge failed");
        }
        var instance = new InstanceView();
        using (var app = new Application(ui => instance.NestedShow(ui)))
        {
            app.Step(); app.Click("button", "Instance increment"); app.Step(); Check(instance.Value == 4, "Private instance bridge failed");
        }
        bool alternate = false;
        using (var app = new Application(ui => { if (alternate) KindViews.Second(ui); else KindViews.First(ui); }))
        {
            app.Step(); Check(KindViews.Value == 3, "First kind failed"); alternate = true; app.Invalidate(); app.Step();
            Check(KindViews.Value == 99, "Component type salt missing");
        }
        long[] rows = [10, 20]; var values = new Dictionary<long, int>();
        using (var app = new Application(ui =>
        {
            foreach (long id in rows) using (ui.Scope(id)) { int observed = 0; SampleViews.Counter(ui, ref observed, id.ToString()); values[id] = observed; }
        }))
        {
            app.Step(); app.Click("button", "Increment 10"); app.Step(); rows = [20, 10]; app.Invalidate(); app.Step();
            Check(values[10] == 1 && values[20] == 0, "Business-keyed reorder lost state");
        }
        int a = 0, b = 0, c = 0, d = 0;
        using (var app = new Application(ui =>
        {
            NestedViews.Parent(ui, ref a, ref b, "A"); NestedViews.Parent(ui, ref c, ref d, "B");
        }))
        {
            app.Step(); app.Click("button", "Increment A left"); app.Step();
            Check(a == 1 && b == 0 && c == 0 && d == 0, "Nested component instances shared state");
        }
        DelegateChecks.Run();
        Console.WriteLine("Interceptor checks passed: unchanged names, same-line calls, ref/default forwarding, private static/instance bridges, remount, type salts and keyed reorder");
    }
}
