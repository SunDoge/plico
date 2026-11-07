using Plico;
namespace Plico.ComponentProbe;

public static partial class SampleViews
{
    public static string Version { get; private set; } = "";
    [Component]
    public static void CounterBody(Ui ui, ref int observed, string title = "Counter")
    {
        Version = "version-one";
        ref int count = ref ui.State(static () => 0);
        using (ui.Column().Gap(12))
        {
            ui.Text($"{title}: {count}");
            if (ui.Button("Increment").Clicked()) count++;
        }
        observed = count;
    }
    public static void Show(Ui ui)
    {
        int observed = 0;
        SampleViews.Counter(ui, ref observed);
    }
}
