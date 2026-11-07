using Plico;
namespace Gallery;

internal sealed partial class GalleryModel
{
    internal int PatternTab, Stars = 3, Copies = 1, Crumb;
    internal double Disk = 42, PriceLow = 200, PriceHigh = 800;
    internal string PatternEmail = "", MailQuery = "", Mailbox = "Inbox", SizeChoice = "Medium";
    internal bool MailOpen = true, GeneralOpen = true, PrivacyOpen, Finding;
    internal string FindQuery = "", Toast = "";
    private double toastUntil;
    private double paneWidth = 200, dotX, dotY;
    private bool birthdayOpen, alertOpen;
    private DateOnly birthday = new(2026, 11, 6), birthdayMonth = new(2026, 11, 1);
    internal int DeletedNotes;
    private string MeetingTime = "15:04";
    private double TintR = 59, TintG = 130, TintB = 246;
    private void Patterns(Ui ui)
    {
        using (ui.Row().Gap(8).Wrap().Shrink())
        {
            foreach (string kind in new[] { "Toast", "Action toast", "Failure", "Sticky" })
                using (ui.Scope(kind))
                    if (ui.Button(kind).Clicked())
                    {
                        Toast = kind == "Failure" ? "Save failed — The disk filled up mid-write." : "Background task completed";
                        toastUntil = kind == "Sticky" ? double.PositiveInfinity : ui.Seconds + 4;
                    }
        }
        if (Toast.Length > 0)
        {
            if (ui.Seconds >= toastUntil) Toast = "";
            else
            {
                using (ui.Row().FillWidth().Padding(12).Gap(12).Background(T.Selection).Radius(8).AlignCenter())
                {
                    ui.Text(Toast).Grow();
                    if (ui.Button("Dismiss notification").Ghost().Clicked()) Toast = "";
                }
                if (double.IsFinite(toastUntil)) ui.RequestAnimationFrame();
            }
        }
        ui.Tabs(["Overview", "Details", "Activity"], ref PatternTab);
        using (Card(ui, new[] { "Overview", "Details", "Activity" }[PatternTab]))
            Hint(ui, new[] { "Click a tab above to switch this panel.", "Each page keeps its application state while another tab is shown.", "Use the toolbar to try transient messages, errors and persistent notifications." }[PatternTab]);
        using (Card(ui, "Navigation and indicators"))
        {
            using (ui.Row().Gap(4).Wrap())
            {
                string[] crumbs = ["Macintosh HD", "Users", "ada", "Documents"];
                for (int i = 0; i < crumbs.Length; i++)
                    using (ui.Scope(i))
                    {
                        if (ui.Button(crumbs[i]).Ghost().Clicked()) Crumb = i;
                        if (i < crumbs.Length - 1) ui.Text("›").Color(T.Muted);
                    }
            }
            ui.Text($"Chosen: {Crumb}").Color(T.Muted);
            ui.Meter("Disk", Disk);
            ui.Slider("Disk used", ref Disk, 0, 100).FillWidth();
            ui.Slider("Minimum price", ref PriceLow, 0, PriceHigh).FillWidth();
            ui.Slider("Maximum price", ref PriceHigh, PriceLow, 1000).FillWidth();
            ui.Text($"${PriceLow:0} – ${PriceHigh:0}").Color(T.Muted);
            using (ui.Row().Gap(8).AlignCenter()) { ui.Spinner(); ui.Text("Loading messages…").Color(T.Muted); }
            ui.Rating(ref Stars);
            ui.Text($"{Stars} of 5 stars").Color(T.Muted);
            ui.Stepper("Copies", ref Copies, 1, 99);
        }
        using (Card(ui, "Sticky header"))
        using (ui.Column().FillWidth().Height(150).ScrollY().Clip())
        {
            using (ui.Column().Sticky().FillWidth().Height(28).Shrink().Background(T.Panel).Padding(4)) ui.Text("Pinned section").Weight(650);
            for (int i = 0; i < 12; i++)
                using (ui.Scope(i))
                using (ui.Row().FillWidth().Height(26).Shrink().Padding(4)) ui.Text($"Scrollable row {i + 1}");
        }
        using (Card(ui, "Resizable split"))
        using (ui.Row().FillWidth().Height(150))
        {
            using (ui.Column().Width((float)paneWidth).FillHeight().Padding(12).Background(T.Control)) ui.Text("Sidebar").Weight(650);
            using (ui.Column().Width(8).FillHeight().Background(T.Border))
            {
                var delta = ui.DragArea("Resize panes");
                paneWidth = Math.Clamp(paneWidth + delta.X, 120, 360);
            }
            using (ui.Column().Width(0).Grow().Padding(12).Gap(8))
            {
                ui.Text("Content").Weight(650);
                ui.Text("Drag the divider to resize the first pane.").FillWidth().Color(T.Muted);
            }
        }
        using (Card(ui, "Source list and search"))
        using (ui.Row().FillWidth().Gap(16).AlignStart())
        {
            using (ui.Column().Width(180).Gap(4))
            {
                if (ui.Button(MailOpen ? "⌄ Mailboxes" : "› Mailboxes").Ghost().Clicked()) MailOpen = !MailOpen;
                if (MailOpen)
                    foreach (string mailbox in new[] { "Inbox", "Sent", "Archive" })
                        using (ui.Scope(mailbox))
                            if ((Mailbox == mailbox ? ui.Button(mailbox).Primary() : ui.Button(mailbox).Ghost()).Clicked()) Mailbox = mailbox;
            }
            using (ui.Column().Width(0).Grow().MinWidth(0).Gap(8))
            {
                ui.TextInput("Search mail", ref MailQuery).FillWidth();
                ui.Text($"Query: {MailQuery}").Color(T.Muted);
                ui.Text($"Mailbox: {Mailbox}").Color(T.Muted);
            }
        }
        using (Card(ui, "Form fields"))
        {
            ui.Text("Email").Weight(650);
            ui.TextInput("Receipt email", ref PatternEmail).FillWidth();
            ui.Text(PatternEmail.Length > 0 && !PatternEmail.Contains('@') ? "Enter an email address." : "For receipts.").Color(PatternEmail.Length > 0 && !PatternEmail.Contains('@') ? 0xdc2626U : T.Muted);
            ui.Stepper("Print copies", ref Copies, 1, 99);
        }
        using (Card(ui, "Links and history"))
        {
            using (ui.Row().Gap(8))
                foreach (string size in new[] { "Small", "Medium", "Large" })
                    using (ui.Scope(size))
                        if ((SizeChoice == size ? ui.Button(size).Primary() : ui.Button(size).Ghost()).Clicked()) SizeChoice = size;
            if (ui.Button("Open the workspace").Ghost().Clicked()) Navigate(Page.Workspace);
            using (ui.Row().Gap(4))
            {
                if (ui.Button("Back").Disabled(historyIndex == 0).Clicked()) Current = history[--historyIndex];
                if (ui.Button("Forward").Disabled(historyIndex >= history.Count - 1).Clicked()) Current = history[++historyIndex];
            }
        }
        using (Card(ui, "Color picker"))
        {
            using (ui.Row().FillWidth().Gap(16))
            {
                using (ui.Column().Width(260).Gap(8))
                {
                    ui.Slider("Red", ref TintR, 0, 255).FillWidth();
                    ui.Slider("Green", ref TintG, 0, 255).FillWidth();
                    ui.Slider("Blue", ref TintB, 0, 255).FillWidth();
                }
                using (ui.Column().Width(140).Height(80).Radius(8).Background(((uint)TintR << 16) | ((uint)TintG << 8) | (uint)TintB)) { }
            }
        }
        using (Card(ui, "Date input"))
            ui.DatePicker("Birthday", ref birthday, ref birthdayOpen, ref birthdayMonth, System.Globalization.CultureInfo.GetCultureInfo("en-GB"));
        using (Card(ui, "Time input"))
        {
            ui.TextInput("Meeting", ref MeetingTime).Width(180);
            ui.Text(TimeOnly.TryParseExact(MeetingTime, "HH:mm", out var time) ? $"Meeting at {time:HH:mm}" : "Use HH:mm, for example 15:04.").Color(T.Muted);
        }
        using (Card(ui, "Alert dialog"))
        {
            if (ui.Button("Delete notes…").Ghost().Clicked()) alertOpen = true;
            ui.Text($"Deleted {DeletedNotes} times").Color(T.Muted);
        }
        if (alertOpen)
            using (ui.Modal(ref alertOpen))
            {
                ui.Text("Delete notes?").FontSize(20).Weight(650);
                ui.Text("You can't undo this.").Color(T.Muted);
                using (ui.Row().FillWidth().JustifyEnd().Gap(8))
                {
                    if (ui.Button("Cancel deletion").Clicked()) alertOpen = false;
                    if (ui.Button("Confirm deletion").Primary().Clicked()) { DeletedNotes++; alertOpen = false; }
                }
            }
        using (Card(ui, "Find bar"))
        {
            if (ui.Button(Finding ? "Close find bar" : "Open find bar").Ghost().Clicked()) Finding = !Finding;
            if (Finding)
            {
                ui.TextInput("Find", ref FindQuery).FillWidth();
                int matches = FindQuery.Length == 0 ? 0 : "the quick brown fox jumps over the lazy dog; the fox naps".Split(FindQuery).Length - 1;
                ui.Text($"{matches} matches").Color(T.Muted);
            }
            Hint(ui, "the quick brown fox jumps over the lazy dog; the fox naps");
        }
        using (Card(ui, "Drag surface"))
        using (ui.Column().FillWidth().Height(120).Radius(8).Background(T.Control))
        {
            using (ui.Column().FillWidth().FillHeight().AlignCenter().JustifyCenter())
            using (ui.Column().Width(14).Height(14).Radius(7).Background(T.Accent).Translate((float)dotX, (float)dotY)) { }
            using (ui.Column().Absolute(0, 0).FillWidth().FillHeight())
            {
                var delta = ui.DragArea();
                dotX = Math.Clamp(dotX + delta.X, -80, 80); dotY = Math.Clamp(dotY + delta.Y, -40, 40);
            }
        }
        using (ui.Column().FillWidth().Shrink().Padding(16).Gap(12).Radius(10).Border(1, T.Border).Background(T.Panel).Selectable())
        {
            ui.Text("Text selection").FontSize(16).Weight(650);
            ui.Text("Drag across these paragraphs; double-click a word, then Ctrl/Cmd+C to copy.").FillWidth();
            ui.Text("中文与日本語 · Selectable text · e\u0301 · 👩‍💻").FontSize(18).FillWidth();
        }
        using (Card(ui, "Disclosure groups"))
        {
            ui.Checkbox("General", ref GeneralOpen);
            if (GeneralOpen) ui.Text("Application preferences and appearance.");
            ui.Checkbox("Privacy", ref PrivacyOpen);
            if (PrivacyOpen) ui.Text("Your files stay on this device.");
        }
    }
}
