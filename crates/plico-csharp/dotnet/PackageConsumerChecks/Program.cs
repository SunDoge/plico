using Plico;
int left = 0, right = 0;
bool mounted = true;
using var app = new Application(ui => {
    if (mounted) { Views.Counter(ui, ref left, "Left"); Views.Counter(ui, ref right, "Right"); }
});
app.Step();
app.Click("button", "Increment Left"); app.Step();
if (left != 1 || right != 0) throw new Exception("Sibling component state collided");
app.Click("button", "Increment Right"); app.Step();
app.Invalidate(); app.Step();
if (left != 1 || right != 1) throw new Exception("State did not survive a rebuild");
mounted = false; app.Invalidate(); app.Step();
mounted = true; app.Invalidate(); app.Step();
if (left != 0 || right != 0) throw new Exception("Unmounted state did not reset");
Console.WriteLine("Package-only consumer passed: isolated local state, events, rebuild, unmount/remount");
public static class Views {
    [Component] public static void Counter(Ui ui, ref int observed, string title) {
        ref int count = ref ui.State(static () => 0);
        using (ui.Column().Gap(12)) {
            ui.Text($"{title}: {count}");
            if (ui.Button($"Increment {title}").Clicked()) count++;
        }
        observed = count;
    }
}
