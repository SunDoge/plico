using Plico;
namespace Gallery;

internal static class GalleryChecks
{
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
    internal static void Run(Application app, GalleryModel model)
    {
        var scene = System.Text.Json.JsonSerializer.Deserialize("{\"page\":\"Markdown\"}", GalleryJsonContext.Default.Scene)!;
        Check(scene.SelectedDate == "2026-11-07" && scene.TimeMs == 1200 && scene.AnimateEffects && scene.Filter == "", "Scene defaults were lost during generated deserialization");
        app.Step(1280, 900);
        foreach (var route in GalleryModel.Routes)
        {
            app.Click("button", route.Label); app.Step(1280, 900);
            Check(model.Current == route.Page, $"Navigation failed for {route.Page}");
        }
        model.Current = Page.Workspace; app.Invalidate(); app.Step(1280, 900);
        // The repeated counters have distinct scopes. Test them via unique labels on Controls.
        model.Current = Page.Controls; app.Invalidate(); app.Step(1280, 900);
        int before = model.Counts[3]; app.Click("button", "Primary"); app.Step(1280, 900);
        Check(model.Counts[3] == before + 1, "Button event was lost or duplicated");
        app.SetText("Full name", "测试・Alex"); app.Step(1280, 900);
        Check(model.Name == "测试・Alex", "Unicode editing failed");
        app.SetValue("Volume", 22); app.Step(1280, 900); Check(model.Volume == 22, "Slider failed");
        app.Click("switch", "Notifications"); app.Step(1280, 900); Check(!model.Notifications, "Switch failed");
        app.Click("switch", "Dark theme"); app.Step(1280, 900); Check(model.Dark, "Theme switch failed");
        app.Click("button", "中文"); app.Step(1280, 900); Check(model.Chinese, "Language toggle failed");
        model.Dark = model.Chinese = false; model.Current = Page.Lists; app.Invalidate(); app.Step(1280, 900);
        app.SetText("Filter documents", "Document 010"); app.Step(1280, 900);
        app.Click("checkbox", "Document 010"); app.Step(1280, 900);
        Check(model.Items.Single(i => i.Id == 10).Done, "Filtered row event reached wrong item");
        app.Click("button", "Descending"); app.Step(1280, 900);
        app.Click("checkbox", "Document 010"); app.Step(1280, 900);
        Check(!model.Items.Single(i => i.Id == 10).Done, "Reordered row lost identity");
        model.Current = Page.Calendar; app.Invalidate(); app.Step(1280, 900);
        app.Click("button", "Next month"); app.Step(1280, 900);
        Check(model.Month.Month == 12, "Month navigation failed");
        app.Click("button", "15"); app.Step(1280, 900);
        Check(model.SelectedDate == new DateOnly(2026, 12, 15), "Date selection failed");
        model.Current = Page.Tasks; app.Invalidate(); app.Step(1280, 900);
        app.Click("button", "Start request"); app.Step(1280, 900);
        Check(model.Request.Status == UiTaskStatus.Running, "Task did not start");
        Program.Pump(app, () => model.Request.Status != UiTaskStatus.Running);
        Check(model.Request.Status == UiTaskStatus.Succeeded, "Task result did not return to UI");
        app.Click("button", "Start request"); app.Step(1280, 900);
        app.Click("button", "Cancel request"); app.Step(1280, 900);
        Check(model.Request.Status == UiTaskStatus.Cancelled, "Task did not cancel");
        model.SimulateFailure = true; app.Invalidate(); app.Step(1280, 900);
        app.Click("button", "Start request"); app.Step(1280, 900);
        Program.Pump(app, () => model.Request.Status != UiTaskStatus.Running);
        Check(model.Request.Status == UiTaskStatus.Failed && model.Request.Error is not null, "Task error was lost");
        using var started = new ManualResetEventSlim();
        var oldResult = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);
        model.Request.Start(_ => { started.Set(); return oldResult.Task; });
        var obsolete = model.Request.Completion;
        Check(started.Wait(TimeSpan.FromSeconds(2)), "Previous request did not start");
        model.Request.Start(_ => Task.FromResult("current"));
        Program.Pump(app, () => model.Request.Status == UiTaskStatus.Succeeded);
        oldResult.SetResult("obsolete");
        Program.Pump(app, () => obsolete.IsCompleted);
        Check(model.Request.Result == "current", "A late previous task overwrote the current result");
        model.Current = Page.Images; model.StartSample(); app.Invalidate();
        Program.Pump(app, () => model.ImageRequest.Status != UiTaskStatus.Running);
        Check(model.ImageRequest.Status == UiTaskStatus.Succeeded && model.ImageRequest.Result != 0, "Image resource did not load");
        model.Current = Page.Motion; app.Invalidate(); app.Step(1280, 900);
        ulong callbacks = app.Stats.Callbacks;
        for (int i = 0; i < 20; i++) app.Step(1280, 900);
        Check(app.Stats.Callbacks == callbacks, "Native-only frames unnecessarily reentered C#");
        app.Click("button", "Expand"); app.Step(1280, 900); Check(model.Expanded, "Motion target did not change");
        model.Current = Page.Game2048; app.Invalidate(); app.Step(1280, 900);
        foreach (var (key, second, destination) in new[] { ("ArrowLeft", 1, 0), ("ArrowRight", 1, 3), ("ArrowUp", 4, 0), ("ArrowDown", 4, 12) })
        {
            model.Board = new int[16]; model.Board[0] = model.Board[second] = 2; model.Score = 0;
            app.Invalidate(); app.Step(1280, 900);
            app.KeyPress(key); app.Step(1280, 900);
            Check(model.Board[destination] == 4 && model.Score == 4, $"2048 key {key} did not merge through the native event route");
        }
        model.Board = [2, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]; model.Score = 0;
        app.Invalidate(); app.Step(1280, 900); app.Click("button", "← Left"); app.Step(1280, 900);
        Check(model.Board[0] == 4 && model.Board[1] == 4 && model.Score == 8, "2048 button merge semantics failed");
        app.Click("button", "New game"); app.Step(1280, 900);
        Check(model.Score == 0 && model.Board.Count(value => value != 0) == 3, "2048 restart failed");
        int[] pausedBoard = (int[])model.Board.Clone();
        model.Current = Page.Controls; app.Invalidate(); app.Step(1280, 900); app.KeyPress("ArrowDown"); app.Step(1280, 900);
        Check(model.Board.SequenceEqual(pausedBoard), "An unmounted game still consumed keys");
        model.Current = Page.DataTable; app.Invalidate(); app.Step(1280, 900);
        app.Click("button", "Size"); app.Step(1280, 900); Check(model.SortBy == "Size" && !model.SortDescending, "Table sorting failed");
        app.Click("button", "Size"); app.Step(1280, 900); Check(model.SortDescending, "Table descending sort failed");
        model.Current = Page.Glass; app.Invalidate(); app.Step(1280, 900);
        app.Click("button", "Save mix"); app.Step(1280, 900); Check(model.SavedMix, "Glass card action failed");
        app.SetValue("Backdrop blur", 24); app.Step(1280, 900); Check(model.GlassBlur == 24, "Backdrop property did not update");
        model.Current = Page.Localization; app.Invalidate(); app.Step(1280, 900);
        app.Click("button", "Add one"); app.Step(1280, 900); Check(model.GreetingCount == 2, "Localized plural count failed");
        model.Current = Page.BlackHole; app.Invalidate(); app.Step(1280, 900);
        app.Click("switch", "Gravitational lens"); app.Step(1280, 900); Check(model.Lens, "Backdrop lens did not enable");
        app.Click("switch", "Gravitational lens"); app.Step(1280, 900); Check(!model.Lens, "Backdrop lens blocked input");
        model.Current = Page.Markdown; app.Invalidate(); app.Step(1280, 900);
        app.Click("button", "Pause arrival"); app.Step(1280, 900); Check(model.ChatPaused, "Streaming pause failed");
        callbacks = app.Stats.Callbacks; app.Step(1280, 900); app.Step(1280, 900);
        Check(app.Stats.Callbacks == callbacks, "Paused stream continued rebuilding");
        app.Click("button", "Send remaining"); app.Step(1280, 900); Check(model.ChatShown > 700, "Streaming burst failed");
        model.Current = Page.Calendar; app.Invalidate(); app.Step(1280, 900);
        app.Click("button", "Choose a date"); app.Step(1280, 900);
        app.Click("button", "2026-12-18"); app.Step(1280, 900);
        Check(model.SelectedDate == new DateOnly(2026, 12, 18), "Calendar popup selection failed");
        model.Current = Page.Patterns; app.Invalidate(); app.Step(1280, 2400);
        app.Click("button", "Details"); app.Step(1280, 2400); Check(model.PatternTab == 1, "Tabs failed");
        app.Click("button", "Rate 4"); app.Step(1280, 2400); Check(model.Stars == 4, "Rating failed");
        app.Click("button", "Increase Copies"); app.Step(1280, 2400); Check(model.Copies == 2, "Stepper failed");
        app.Click("button", "Toggle navigation"); app.Step(1280, 2400); Check(model.Collapsed, "Sidebar collapse failed");
        app.Click("button", "Toggle navigation"); app.Step(1280, 2400); Check(!model.Collapsed, "Sidebar expand failed");
        model.Current = Page.ComponentKit; app.Invalidate(); app.Step(1280, 1200);
        app.Click("button", "Navigation"); app.Step(1280, 1200);
        app.Click("button", "Recent"); app.Step(1280, 1200); Check(model.KitSegment == 1, "Kit segment failed");
        app.Click("button", "Next page"); app.Step(1280, 1200); Check(model.KitPage == 2, "Kit pagination failed");
        app.Click("button", "Actions…"); app.Step(1280, 1200);
        app.Click("button", "Archive"); app.Step(1280, 1200); Check(model.KitMenuResult == "Archive", "Kit menu failed");
        app.Click("button", "Forms"); app.Step(1280, 1200);
        app.SetText("Kit receipt email", "kit@example.com"); app.Step(1280, 1200); Check(model.KitEmail == "kit@example.com", "Kit field failed");
        app.Click("button", "Corners"); app.Step(1280, 1200);
        app.SetValue("Corner smoothing", .5); app.Step(1280, 1200);
        Check(model.KitTab == 3 && model.CornerAmount == .5, "Corner smoothing control failed");
        using (var lyrics = new GalleryModel { Current = Page.Lyrics, LyricStart = 10 })
        using (var player = new Application(lyrics.Show, virtualTime: true))
        {
            lyrics.Attach(player);
            player.SetTime(0); player.Step(1280, 900);
            player.SetTime(500); player.Step(1280, 900);
            Check(Math.Abs(lyrics.LyricPosition - 10.5) < .001, "Lyrics did not follow virtual clock");
            player.Click("button", "Pause lyrics"); player.Step(1280, 900);
            double paused = lyrics.LyricPosition;
            player.SetTime(2000); player.Step(1280, 900);
            Check(!lyrics.LyricPlaying && lyrics.LyricPosition == paused, "Paused lyrics advanced");
            player.SetValue("Lyric playback position", 7); player.Step(1280, 900);
            Check(lyrics.LyricPosition == 7, "Lyric seeking failed");
            player.Click("button", "Replay"); player.Step(1280, 900);
            Check(lyrics.LyricPosition == 0, "Lyric replay failed");
            player.Click("button", "Play lyrics"); player.Step(1280, 900);
            player.SetTime(3000); player.Step(1280, 900);
            Check(Math.Abs(lyrics.LyricPosition - 1) < .001, "Lyrics did not resume from current position");
        }
        Console.WriteLine("Gallery interaction checks passed");
    }
}
