using Plico;
namespace Plico.InterceptorProbe;

// Public/internal components do not need partial or a Body suffix.
public static class SampleViews
{
    public static string Version { get; private set; } = "";
    [Component]
    public static void Counter(Ui ui, ref int observed, string title = "Counter")
    {
        Version = "version-one";
        ref int count = ref ui.State(static () => 0);
        using (ui.Column().Gap(12))
        {
            ui.Text($"{title}: {count}");
            if (ui.Button($"Increment {title}").Clicked()) count++;
        }
        observed = count;
    }
    public static void Show(Ui ui)
    {
        int observed = 0;
        SampleViews.Counter(ui, ref observed);
    }
}
