using Plico;
namespace Gallery;

internal sealed partial class GalleryModel
{
    internal UiTask<string> Request = null!;
    internal bool SimulateFailure;
    internal int TaskRequests;
    internal void Attach(Application application)
    {
        App = application;
        Request = new(application);
        ImageRequest = new(application);
    }
    private void Tasks(Ui ui)
    {
        using (Card(ui, Tr("Fetch a translation", "获取翻译")))
        {
            ui.Text("A small request with a real asynchronous lifecycle.").FillWidth();
            ui.Checkbox("Simulate a service error", ref SimulateFailure);
            using (ui.Row().Gap(10).AlignCenter().Wrap())
            {
                if (ui.Button(Request.Status == UiTaskStatus.Running ? "Restart request" : "Start request").Primary().Clicked())
                {
                    bool fail = SimulateFailure;
                    int request = ++TaskRequests;
                    Request.Start(async token =>
                    {
                        await Task.Delay(800, token).ConfigureAwait(false);
                        if (fail) throw new InvalidOperationException("The sample service is unavailable. Try again with error simulation off.");
                        return $"Request #{request}: こんにちは → 你好，世界。";
                    });
                }
                if (ui.Button("Cancel request").Disabled(Request.Status != UiTaskStatus.Running).Clicked()) Request.Cancel();
                if (Request.Status == UiTaskStatus.Running) ui.Spinner();
                ui.Text(Request.Status.ToString()).Color(T.Muted);
            }
            Rule(ui);
            switch (Request.Status)
            {
                case UiTaskStatus.Idle: Hint(ui, "Start a request to see loading, completion and cancellation."); break;
                case UiTaskStatus.Running: Hint(ui, "Waiting for the service. You can visit another page while it runs."); break;
                case UiTaskStatus.Succeeded: ui.Text(Request.Result ?? "").FontSize(20).FillWidth(); break;
                case UiTaskStatus.Failed: ui.Text(Request.Error?.Message ?? "Request failed").Color(0xdc2626).FillWidth(); break;
                case UiTaskStatus.Cancelled: Hint(ui, "Cancelled. A late result will not replace the next request."); break;
            }
        }
        using (Card(ui, Tr("Where the work runs", "任务在哪里执行")))
        {
            ui.Text("Task execution → background worker").Weight(600);
            ui.Text("Completion → UI dispatcher → state update → declaration").FillWidth().Color(T.Muted);
            Hint(ui, "No timer polls the service. Native wake notifications bring completed work back to the UI.");
        }
    }
}
