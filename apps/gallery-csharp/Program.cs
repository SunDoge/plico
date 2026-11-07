using Plico;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Gallery;

internal sealed record Scene
{
    public string Page { get; set; } = "Workspace";
    public bool Dark { get; set; }
    public bool Chinese { get; set; }
    public bool Compact { get; set; }
    public bool Collapsed { get; set; }
    public bool Large { get; set; }
    public ulong TimeMs { get; set; } = 1200;
    public bool Lens { get; set; }
    public double? PointerX { get; set; }
    public double? PointerY { get; set; }
    public bool CompleteReply { get; set; }
    public string Filter { get; set; } = "";
    public string SelectedDate { get; set; } = "2026-11-07";
    public bool Expanded { get; set; }
    public int Effect { get; set; }
    public bool AnimateEffects { get; set; } = true;
}

internal static class Program
{
    [STAThread]
    public static int Main(string[] args)
    {
        try
        {
            string? capture = Option(args, "--capture"), all = Option(args, "--capture-all"), sceneFile = Option(args, "--scene");
            Scene scene = sceneFile is null ? new() : JsonSerializer.Deserialize(File.ReadAllText(sceneFile), GalleryJsonContext.Default.Scene)
                ?? throw new ArgumentException("Empty scene");
            using var model = new GalleryModel
            {
                Current = ParsePage(Option(args, "--page") ?? scene.Page),
                Dark = args.Contains("--dark") || scene.Dark,
                Chinese = args.Contains("--zh") || scene.Chinese,
                Compact = scene.Compact,
                Collapsed = scene.Collapsed,
                Large = scene.Large,
                Lens = scene.Lens,
                Filter = scene.Filter,
                SelectedDate = DateOnly.ParseExact(scene.SelectedDate, "yyyy-MM-dd"),
                Expanded = scene.Expanded,
                EffectIndex = Math.Clamp(scene.Effect, 0, 2),
                AnimateEffects = scene.AnimateEffects
            };
            model.Month = new(model.SelectedDate.Year, model.SelectedDate.Month, 1);
            using var app = new Application(model.Show, virtualTime: capture is not null || all is not null);
            if (capture is not null || all is not null) app.SetTime(scene.TimeMs);
            if (scene.CompleteReply) model.ChatShown = int.MaxValue;
            model.Attach(app);
            if (args.Contains("--self-test"))
            {
                GalleryChecks.Run(app, model);
                Console.WriteLine(JsonSerializer.Serialize(new GalleryResult("passed", Enum.GetValues<Page>().Length, app.Stats), GalleryJsonContext.Default.GalleryResult));
            }
            else if (all is not null)
            {
                Directory.CreateDirectory(all);
                model.StartSample(); Pump(app, () => model.ImageRequest.Status != UiTaskStatus.Running);
                foreach (Page page in Enum.GetValues<Page>())
                {
                    model.Current = page; app.Invalidate();
                    if (page == Page.Markdown) { model.ChatShown = int.MaxValue; model.ChatPaused = true; }
                    app.Capture(Path.Combine(all, $"{page}.png"), 1280, 900);
                    Console.WriteLine($"Captured {page}");
                }
                model.Current = Page.Controls; model.Dark = true; app.Invalidate();
                app.Capture(Path.Combine(all, "Controls-dark.png"), 1280, 900);
                model.Dark = false; app.Invalidate();
                app.Capture(Path.Combine(all, "Controls-narrow.png"), 760, 900);
            }
            else if (capture is not null)
            {
                if (model.Current == Page.Images) { model.StartSample(); Pump(app, () => model.ImageRequest.Status != UiTaskStatus.Running); }
                if (scene.PointerX is double px && scene.PointerY is double py) { app.Step(1280, 900); app.PointerMove(px, py); app.Step(1280, 900); }
                app.Capture(capture, 1280, 900);
                Console.WriteLine($"Captured {capture}");
            }
            else
            {
                model.StartSample();
                app.Run("Plico gallery · C#", 1280, 900);
            }
            return 0;
        }
        catch (Exception error) { Console.Error.WriteLine(error); return 1; }
    }
    internal static void Pump(Application app, Func<bool> done)
    {
        var until = System.Diagnostics.Stopwatch.StartNew();
        do
        {
            app.Step(1280, 900);
            if (until.Elapsed.TotalSeconds > 10) throw new TimeoutException("UI operation did not complete");
            Thread.Sleep(5);
        } while (!done());
        app.Step(1280, 900);
    }
    private static Page ParsePage(string value) => Enum.TryParse<Page>(value.Replace("-", ""), true, out var page) && Enum.IsDefined(page)
        ? page : throw new ArgumentException($"Unknown page: {value}. Use {string.Join(", ", Enum.GetNames<Page>())}");
    private static string? Option(string[] args, string option)
    {
        int index = Array.IndexOf(args, option);
        if (index < 0) return null;
        if (index + 1 >= args.Length || args[index + 1].StartsWith("--")) throw new ArgumentException($"Missing value for {option}");
        return args[index + 1];
    }
}
