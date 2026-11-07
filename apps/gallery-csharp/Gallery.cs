using Plico;

namespace Gallery;

internal enum Page { Workspace, Layout, Controls, Motion, Lists, DataTable, Tree, Calendar, Images, Tasks, Effects, Game2048, Markdown, Glass, Localization, BlackHole, Patterns }

internal readonly record struct Palette(uint Surface, uint Panel, uint Foreground, uint Muted, uint Border, uint Control, uint Accent, uint Selection)
{
    internal static Palette Light => new(0xf7f7f8, 0xffffff, 0x202024, 0x71717a, 0xd9d9de, 0xf4f4f5, 0x285bd6, 0xdde9ff);
    internal static Palette Dark => new(0x18181b, 0x202023, 0xf4f4f5, 0xa1a1aa, 0x3f3f46, 0x27272a, 0x94b5ff, 0x263d65);
}

internal sealed partial class GalleryModel : IDisposable
{
    internal Application App = null!;
    internal Page Current;
    internal bool Dark, Chinese, Compact, Large, Collapsed;
    internal readonly Dictionary<Page, int> Visits = new();
    internal Palette T => Dark ? Palette.Dark : Palette.Light;
    internal string Tr(string english, string chinese) => Chinese ? chinese : english;
    internal static readonly (Page Page, string Label, string Chinese, string Icon)[] Routes = [
        (Page.Workspace, "Workspace", "工作区", "apps"),
        (Page.Layout, "Layout", "布局", "home"), (Page.Controls, "Controls", "控件", "settings"),
        (Page.Lists, "Lists", "列表", "text_bullet_list_square"), (Page.DataTable, "Data table", "数据表格", "text_bullet_list_square"),
        (Page.Tree, "Tree navigation", "树形导航", "home"), (Page.Calendar, "Calendar", "日历", "calendar"),
        (Page.Localization, "Localization", "本地化", "chat"), (Page.Patterns, "Desktop patterns", "桌面模式", "settings"),
        (Page.Motion, "Motion", "动效", "design_ideas"), (Page.Glass, "Glass", "毛玻璃", "image"),
        (Page.BlackHole, "Black hole", "黑洞", "image"), (Page.Effects, "Effects", "特效", "design_ideas"),
        (Page.Images, "Images", "图片", "image"), (Page.Markdown, "Markdown chat", "Markdown 聊天", "chat"),
        (Page.Game2048, "2048", "2048", "puzzle_piece"), (Page.Tasks, "Async tasks", "异步任务", "chat")
    ];
    private static readonly Dictionary<string, string> resources = new();
    private static string Resource(string name)
    {
        if (!resources.TryGetValue(name, out var value))
        {
            using var stream = typeof(GalleryModel).Assembly.GetManifestResourceStream("Plico.Gallery." + name)
                ?? throw new InvalidOperationException($"Missing gallery resource {name}");
            using var reader = new StreamReader(stream); resources[name] = value = reader.ReadToEnd();
        }
        return value;
    }
    private static string Icon(string name) => name switch
    {
        "home" => "<path d='m3 10 9-7 9 7v10H3Z'/><path d='M9 20v-7h6v7'/>",
        "layout" => "<rect x='3' y='3' width='18' height='18' rx='2'/><path d='M9 3v18M9 9h12'/>",
        "sliders" => "<path d='M4 7h16M4 17h16'/><circle cx='8' cy='7' r='2'/><circle cx='16' cy='17' r='2'/>",
        "motion" => "<path d='M3 12h5l4-8 4 16 3-8h2'/>",
        "list" => "<path d='M9 6h12M9 12h12M9 18h12M3 6h1M3 12h1M3 18h1'/>",
        "table" => "<rect x='3' y='3' width='18' height='18' rx='2'/><path d='M3 9h18M9 9v12M3 15h18'/>",
        "tree" => "<path d='M5 3v14h6M5 7h6M15 3h6v7h-6ZM15 14h6v7h-6Z'/>",
        "calendar" => "<rect x='3' y='5' width='18' height='16' rx='2'/><path d='M7 3v4M17 3v4M3 11h18M7 15h2M13 15h2'/>",
        "image" => "<rect x='3' y='3' width='18' height='18' rx='2'/><circle cx='8' cy='8' r='1.5'/><path d='m3 17 5-5 4 4 4-6 5 7'/>",
        "cloud" => "<path d='M6 17a4 4 0 0 1-1-8 7 7 0 0 1 13-1 5 5 0 0 1 0 10M12 12v9m-3-3 3 3 3-3'/>",
        "spark" => "<path d='m12 2 3 7 7 3-7 3-3 7-3-7-7-3 7-3Z'/>",
        _ => "<rect x='3' y='3' width='7' height='7' rx='1'/><rect x='14' y='3' width='7' height='7' rx='1'/><rect x='3' y='14' width='7' height='7' rx='1'/><rect x='14' y='14' width='7' height='7' rx='1'/>"
    };
    private static string SvgIcon(string name) => $"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.7' stroke-linecap='round' stroke-linejoin='round'>{Icon(name)}</svg>";

    public void Show(Ui ui)
    {
        ui.DarkTheme = Dark;
        ui.Theme = Dark ? Plico.Theme.Dark : Plico.Theme.Light;
        ui.DefaultFontSize = Large ? 18U : 14U;
        using (ui.Row().FillWidth().FillHeight().Background(T.Surface))
        {
            Sidebar(ui);
            using (ui.Column().Width(0).Grow().MinWidth(0).FillHeight().Padding(Compact ? 18 : 24).Gap(16))
            {
                using (ui.Row().FillWidth().Shrink().AlignCenter().SpaceBetween())
                {
                    ui.Text(Tr("Workspace / Gallery", "工作区 / 展示")).FontSize(12).Color(T.Muted);
                    using (ui.Row().Gap(16).AlignCenter())
                    {
                        ui.Switch(Tr("Dark theme", "深色主题"), ref Dark);
                        ui.Checkbox(Tr("Large text", "大号文字"), ref Large);
                    }
                }
                using (ui.Column(key: "page-" + Current).FillWidth().Height(0).Grow().MinHeight(0).ScrollY(Current is not (Page.Markdown or Page.Glass)).Gap(16))
                {
                    var route = Routes.First(r => r.Page == Current);
                    ui.Text(Current == Page.Glass ? Tr("Glass in motion", "流动的玻璃") : Chinese ? route.Chinese : route.Label).FontSize(28).Weight(750).Shrink();
                    ui.Text(Subtitle()).Color(T.Muted).FillWidth().Shrink();
                    using (ui.Scope(Current.ToString()))
                    {
                        switch (Current)
                        {
                            case Page.Workspace: Workspace(ui); break;
                            case Page.Layout: Layout(ui); break;
                            case Page.Controls: Controls(ui); break;
                            case Page.Motion: Motion(ui); break;
                            case Page.Lists: Lists(ui); break;
                            case Page.DataTable: DataTable(ui); break;
                            case Page.Tree: Tree(ui); break;
                            case Page.Calendar: Calendar(ui); break;
                            case Page.Images: Images(ui); break;
                            case Page.Tasks: Tasks(ui); break;
                            case Page.Effects: Effects(ui); break;
                            case Page.Game2048: Game(ui); break;
                            case Page.Markdown: MarkdownChat(ui); break;
                            case Page.Glass: Glass(ui); break;
                            case Page.Localization: Localization(ui); break;
                            case Page.BlackHole: BlackHole(ui); break;
                            case Page.Patterns: Patterns(ui); break;
                        }
                    }
                }
            }
        }
        BlackHoleOverlay(ui);
    }
    private string Subtitle() => Current switch
    {
        Page.Markdown => Tr("A reply that arrives at its own pace.", "以自己的节奏到来的回复。"),
        Page.Glass => Tr("A soft backdrop. A clear foreground.", "柔和的背景，清晰的前景。"),
        Page.Localization => Tr("Messages, plurals, numbers and dates follow the selected language.", "消息、复数、数字和日期随所选语言变化。"),
        Page.BlackHole => Tr("A small lens at your pointer. Toggle it on, then move across any page.", "鼠标位置的小型引力透镜，可在任意页面启用。"),
        Page.Patterns => Tr("Tabs, toolbars, toasts and desktop interactions.", "标签页、工具栏、通知与桌面交互。"),
        Page.Workspace => Tr("A small workspace, built with ordinary C# components.", "使用普通 C# 组件构建的工作区。"),
        Page.Layout => Tr("Flexible columns, wrapping rows, and nested scrolling.", "弹性分栏、自动换行和嵌套滚动。"),
        Page.Controls => Tr("Native text editing. Simple state. Familiar controls.", "原生文字编辑、普通状态和常用控件。"),
        Page.Motion => Tr("Set a new target. Let the native runtime carry the motion.", "设置新目标，交给原生运行时完成动画。"),
        Page.Lists => Tr("Filter, reorder, and edit rows without losing their identity.", "筛选、重排与编辑，保留每一项的身份。"),
        Page.DataTable => Tr("Sortable data with a header that stays in view.", "支持排序，表头始终可见。"),
        Page.Tree => Tr("Nested folders and a small document inspector.", "嵌套目录与文档预览。"),
        Page.Calendar => Tr("A calendar made from reusable C# components.", "用可复用 C# 组件构建的日历。"),
        Page.Images => Tr("Decode in the background. Render through native image resources.", "后台解码，通过原生图片资源显示。"),
        Page.Tasks => Tr("Loading, cancellation, errors, and results.", "加载、取消、错误与结果。"),
        Page.Effects => Tr("WGSL runs on the GPU; C# only declares the surface.", "WGSL 在 GPU 执行，C# 只声明画面。"),
        _ => Tr("A familiar puzzle. Ordinary C# state and control flow.", "熟悉的数字游戏，普通的 C# 状态和控制流。")
    };
    private void Sidebar(Ui ui)
    {
        using (ui.Column().Width(Collapsed ? 64 : 224).Shrink().FillHeight().Padding(10).Gap(12).Background(T.Panel).Clip().Motion(milliseconds: 240))
        {
            using (ui.Row().FillWidth().Height(36).Gap(8).AlignCenter())
            {
                if (!Collapsed) { ui.Svg(Resource("Logo.svg"), 24); ui.Text("plico").FontSize(20).Weight(650).Grow(); }
                if (ui.Button(Collapsed ? "›" : "‹").AriaLabel("Toggle navigation").Ghost().Clicked()) Collapsed = !Collapsed;
            }
            using (ui.Column().Height(0).Grow().FillWidth().ScrollY().Gap(2))
                foreach (var route in Routes)
                    using (ui.Scope(route.Page.ToString()))
                    {
                        if (!Collapsed && route.Page is Page.Workspace or Page.Layout or Page.Motion)
                            ui.Text(route.Page == Page.Workspace ? Tr("Workspace", "工作区") : route.Page == Page.Layout ? Tr("Components", "组件") : Tr("Experiments", "实验"))
                                .FontSize(12).Weight(600).Color(T.Muted).Padding(8);
                        string label = Chinese ? route.Chinese : route.Label;
                        using var item = ui.Box(key: "navigate").Interactive(label).FillWidth().Height(34).Padding(8).Gap(8).AlignCenter()
                            .Radius(6).Background(Current == route.Page ? T.Selection : T.Panel).Color(Current == route.Page ? T.Accent : T.Foreground)
                            .HoverBackground(T.Control).PressedBackground(T.Selection);
                        ui.Svg(Resource("Icons." + route.Icon + ".svg"), 18);
                        if (!Collapsed) ui.Text(label).FontSize(14).Shrink();
                        if (item.Clicked()) Navigate(route.Page);
                    }
            if (!Collapsed)
            {
                using (ui.Row().Gap(6))
                {
                    if ((Chinese ? ui.Button("English").Ghost() : ui.Button("English").Primary()).Clicked()) Chinese = false;
                    if ((Chinese ? ui.Button("中文").Primary() : ui.Button("中文").Ghost()).Clicked()) Chinese = true;
                }
                ui.Checkbox(Tr("Compact", "紧凑"), ref Compact);
                ui.Text("Rust, in motion.").FontSize(12).Weight(600);
                ui.Text(Tr("Development gallery", "开发展示")).FontSize(12).Color(T.Muted);
            }
        }
    }
    private readonly List<Page> history = [Page.Workspace];
    private int historyIndex;
    internal void Navigate(Page page)
    {
        Current = page; Visits[page] = Visits.GetValueOrDefault(page) + 1;
        if (history[historyIndex] == page) return;
        if (historyIndex + 1 < history.Count) history.RemoveRange(historyIndex + 1, history.Count - historyIndex - 1);
        history.Add(page); historyIndex++;
    }
    private Ui.Container Card(Ui ui, string title)
    {
        var card = ui.Column().FillWidth().Shrink().Padding(22).Gap(16).Radius(12).Border(1, T.Border).Background(T.Panel);
        ui.Text(title).FontSize(17).Weight(650);
        return card;
    }
    private void Hint(Ui ui, string value) => ui.Text(value).Color(T.Muted).FontSize(13).FillWidth();
    private void Rule(Ui ui) { using (ui.Column().FillWidth().Height(1).Background(T.Border)) { } }
    private void Progress(Ui ui, double value, string label)
    {
        using (ui.Column().FillWidth().Gap(8))
        {
            using (ui.Row().FillWidth().SpaceBetween()) { ui.Text(label); ui.Text($"{value:P0}").Color(T.Muted); }
            using (ui.Row().FillWidth().Height(7).Radius(4).Background(T.Control))
            {
                using (ui.Column().WidthPercent((float)Math.Clamp(value, 0, 1) * 100).Height(7).Radius(4).Background(T.Accent).Motion()) { }
            }
        }
    }
    public void Dispose() { Request?.Dispose(); ImageRequest?.Dispose(); }
}
