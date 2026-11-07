using Plico;

namespace Gallery;

internal sealed partial class GalleryModel
{
    internal int[] Counts = [0, 10, 0, 100];
    internal string Name = "Alex Morgan", Email = "alex@example.com", Message = "你好，Plico。";
    internal bool Notifications = true, Autosave = true, Analytics;
    internal string Choice = "Balanced";
    internal double Volume = 65, Quality = 80;
    internal bool Expanded;
    private static readonly string[] Choices = ["Fast", "Balanced", "Detailed"];

    private void Workspace(Ui ui)
    {
        using (Card(ui, Tr("Your workspace", "你的工作区")))
        {
            using (ui.Row().FillWidth().AlignCenter().SpaceBetween().Wrap().Gap(16))
            {
                using (ui.Column().Gap(8))
                {
                    ui.Text(Tr("Good morning, Alex.", "早上好，Alex。")).FontSize(24).Weight(650);
                    Hint(ui, Tr("Everything you need, in one place.", "常用操作与最近的项目。"));
                }
                if (ui.Button(Tr("Create project", "新建项目")).Primary().Clicked()) Current = Page.Lists;
            }
            Rule(ui);
            foreach (var project in new[] { ("Manga reader", "Updated just now", "Ready"), ("Translation workflow", "Updated 12 minutes ago", "In progress"), ("Interface experiments", "Updated yesterday", "Draft") })
            {
                using (ui.Scope(project.Item1))
                using (ui.Row().FillWidth().AlignCenter().Gap(14))
                {
                    ui.Svg(SvgIcon("layout"), 22).Color(T.Accent);
                    using (ui.Column().Width(0).Grow().Gap(4))
                    {
                        ui.Text(project.Item1).Weight(600);
                        ui.Text(project.Item2).FontSize(13).Color(T.Muted);
                    }
                    ui.Text(project.Item3).Color(T.Muted);
                    if (ui.Button("Open").Ghost().Clicked()) Current = Page.Controls;
                }
            }
        }
        ui.Text(Tr("Reusable counters", "可复用的计数器")).FontSize(20).Weight(650);
        using (ui.Row().FillWidth().Gap(16).Wrap())
        {
            using (ui.Column().Width(0).Grow().MinWidth(240))
                LocalCounter(ui, "Local counter 1");
            using (ui.Column().Width(0).Grow().MinWidth(240))
                LocalCounter(ui, "Local counter 2");
            using (ui.Column().Width(0).Grow().MinWidth(240))
                Counter(ui, "App counter", ref Counts[2]);
        }
        Hint(ui, Tr("Local counters reset when you leave this page; the app counter survives navigation.", "离开页面后局部计数器会重置，应用计数器会保留。"));
    }
    [Component]
    private void LocalCounter(Ui ui, string title)
    {
        ref int count = ref ui.State(static () => 0);
        Counter(ui, title, ref count);
    }
    private void Counter(Ui ui, string title, ref int count)
    {
        using (Card(ui, title))
        {
            ui.Text(count.ToString()).FontSize(32).Weight(650);
            using (ui.Row().Gap(6).Wrap())
            {
                if (ui.Button("−").AriaLabel($"Decrement {title}").Ghost().Clicked()) count--;
                if (ui.Button("+").AriaLabel($"Increment {title}").Primary().Clicked()) count++;
                if (ui.Button("Reset").AriaLabel($"Reset {title}").Clicked()) count = 0;
            }
        }
    }
    private void Controls(Ui ui)
    {
        using (Card(ui, Tr("Buttons & selection", "按钮与选择")))
        {
            using (ui.Row().Gap(10).Wrap().AlignCenter())
            {
                if (ui.Button("Primary").Primary().Clicked()) Counts[3]++;
                if (ui.Button("Secondary").Clicked()) Counts[3]--;
                if (ui.Button("Ghost").Ghost().Clicked()) Counts[3] = 0;
                ui.Button("Disabled").Disabled();
                using (var composed = ui.ButtonContent("Save with icon", primary: true))
                {
                    ui.Text("↓"); ui.Text("Save");
                    if (composed.Clicked()) Counts[3]++;
                }
                ui.Text($"Value: {Counts[3]}").Color(T.Muted);
            }
            Rule(ui);
            using (ui.Row().Gap(24).Wrap().AlignCenter())
            {
                ui.Switch("Notifications", ref Notifications);
                ui.Checkbox("Autosave", ref Autosave);
                ui.Checkbox("Share analytics", ref Analytics);
            }
            using (ui.Row().Gap(8).Wrap())
                foreach (string option in Choices)
                    using (ui.Scope(option))
                        if ((Choice == option ? ui.Button(option).Primary() : ui.Button(option).Ghost()).Clicked()) Choice = option;
            Hint(ui, $"Selected: {Choice}");
        }
        using (Card(ui, Tr("Text fields", "文字输入")))
        {
            using ((ui.Width > 1000 ? ui.Row() : ui.Column()).FillWidth().Gap(20))
            {
                using ((ui.Width > 1000 ? ui.Column().Width(0).Grow() : ui.Column().FillWidth()).MinWidth(180).Gap(8))
                {
                    ui.Text("Full name").Weight(600);
                    ui.TextInput("Full name", ref Name).Placeholder("Your full name").PaddingX(12).FillWidth();
                }
                using ((ui.Width > 1000 ? ui.Column().Width(0).Grow() : ui.Column().FillWidth()).MinWidth(180).Gap(8))
                {
                    ui.Text("Email address").Weight(600);
                    ui.TextInput("Email address", ref Email).FillWidth();
                }
            }
            ui.TextInput("Message", ref Message).FillWidth();
            ui.Text($"{Name} · {Email}").Color(T.Muted).FillWidth();
            Hint(ui, "Selection, clipboard and IME use Plico's native text input.");
        }
        using (Card(ui, Tr("Sliders", "滑块")))
        {
            ui.Text($"Volume · {Volume:0}%").Weight(600);
            ui.Slider("Volume", ref Volume, 0, 100).FillWidth();
            ui.Text($"Quality · {Quality:0}%").Weight(600);
            ui.Slider("Quality", ref Quality, 0, 100).FillWidth();
        }
    }
    private void Layout(Ui ui)
    {
        using (Card(ui, Tr("Flexible space", "弹性空间")))
        {
            Hint(ui, "The middle column grows. Resize the window to see the available space redistribute.");
            using (ui.Row().FillWidth().Height(150).Gap(12))
            {
                Tile(ui, "Fixed · 120", 120, 0, T.Selection);
                Tile(ui, "Grow · 1", 0, 1, T.Control);
                Tile(ui, "Grow · 2", 0, 2, T.Selection);
            }
        }
        using (Card(ui, Tr("Wrapping & alignment", "换行与对齐")))
        {
            using (ui.Row().FillWidth().Gap(10).Wrap().AlignCenter())
                foreach (string word in new[] { "Taffy", "C#", "Rust", "普通函数", "Stable identity", "Hot reload", "Native text", "WGSL", "布局" })
                    using (ui.Scope(word))
                    using (ui.Column().Padding(12).Radius(6).Background(T.Control))
                        ui.Text(word);
        }
        using (Card(ui, Tr("A nested scroll viewport", "嵌套滚动区域")))
        {
            using (ui.Column().FillWidth().Height(180).ScrollY().Gap(8).Padding(12).Background(T.Panel).Radius(8))
                for (int i = 1; i <= 24; i++)
                    using (ui.Scope(i)) ui.Text($"Row {i:00}  /  Independent scrolling inside the page");
        }
    }
    private void Tile(Ui ui, string label, float width, float grow, uint color)
    {
        using (ui.Column().Width(width).Grow(grow).MinWidth(0).FillHeight().Padding(16).Radius(8).Background(color))
            ui.Text(label).Weight(600).FillWidth();
    }
    private void Motion(Ui ui)
    {
        using (Card(ui, Tr("Property transitions", "属性过渡")))
        {
            if (ui.Button(Expanded ? "Collapse" : "Expand").Primary().Clicked()) Expanded = !Expanded;
            using (ui.Row().FillWidth().Height(110).AlignCenter().Background(T.Panel).Radius(8).Padding(12))
            {
                using (ui.Column().Width(Expanded ? 420 : 160).Height(70).Padding(20).Radius(Expanded ? 24 : 8)
                    .Background(Expanded ? T.Accent : T.Selection).Motion())
                    ui.Text(Expanded ? "Expanded" : "Compact").Color(Expanded && !Dark ? 0xffffff : T.Foreground);
            }
            Hint(ui, "Width, radius and color transition in Rust. The C# component runs only when the target changes.");
        }
        using (Card(ui, Tr("Native activity indicators", "原生活动指示")))
        {
            using (ui.Row().Gap(20).AlignCenter())
            {
                for (int i = 0; i < 4; i++)
                    using (ui.Scope(i)) ui.Spinner().Width(20 + i * 5).Height(20 + i * 5);
                ui.Text("Working in the background").Color(T.Muted);
            }
            Progress(ui, Quality / 100, "Completion");
            ui.Slider("Completion", ref Quality, 0, 100).FillWidth();
        }
    }
}
