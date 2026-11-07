using Plico;
namespace Gallery;

internal sealed partial class GalleryModel
{
    internal double CornerAmount = 1;
    internal int KitTab, KitSegment, KitPage = 1, KitCrumb;
    internal bool KitDisclosure = true, KitChip = true, KitTag = true;
    internal string KitQuery = "", KitEmail = "", KitMenuResult = "No action yet";
    internal double KitCopies = 2;
    internal TimeOnly KitTime = new(15, 30);
    internal uint KitColor = 0x285bd6;
    private static readonly string[] KitTabs = ["Surfaces & feedback", "Navigation", "Forms", "Corners"];
    private static readonly SelectOption[] KitCommands = [new("duplicate", "Duplicate"), new("archive", "Archive"), new("delete", "Delete", true)];

    [Component]
    private void ComponentKit(Ui ui)
    {
        ref bool notice = ref ui.State(static () => true);
        ui.Tabs(KitTabs, ref KitTab);
        if (KitTab == 0)
        {
            ui.Notification("All changes are saved on this device.", ref notice, Tone.Success);
            using ((ui.Width > 1000 ? ui.Row() : ui.Column()).FillWidth().Gap(16).AlignStart().Shrink())
            {
                using ((ui.Width > 1000 ? ui.Column().Width(0).Grow() : ui.Column().FillWidth()).MinWidth(0).Gap(16).Shrink())
                {
                    using (ui.Card("People & status", "Small components, composed in C#."))
                    {
                        using (ui.Row().Gap(12).AlignCenter())
                        {
                            ui.Avatar("Alex Morgan", size: 44);
                            using (ui.Column().Gap(4))
                            {
                                ui.Text("Alex Morgan").Weight(650);
                                ui.Text("Personal workspace").Color(T.Muted);
                            }
                        }
                        ui.Divider();
                        using (ui.Row().Gap(8).Wrap())
                        {
                            ui.Badge("Draft"); ui.Badge("Synced", Tone.Success); ui.Badge("Review", Tone.Warning);
                        }
                        using (ui.Row().Gap(8).AlignCenter().Wrap())
                        {
                            ui.Chip("Include archived", ref KitChip);
                            if (KitTag && ui.Tag("Design files")) KitTag = false;
                        }
                        if (!KitTag && ui.Button("Restore tag").Ghost().Clicked()) KitTag = true;
                    }
                    ui.Alert("You're up to date", "Background work returns through the application dispatcher.", Tone.Success);
                    ui.Alert("Connection interrupted", "Your local changes are safe. Try again when you're online.", Tone.Warning);
                }
                using ((ui.Width > 1000 ? ui.Column().Width(0).Grow() : ui.Column().FillWidth()).MinWidth(0).Gap(16).Shrink())
                {
                    using (ui.Card("Loading preview", "The shimmer runs in WGSL without a managed frame callback."))
                    {
                        // The native animation clock drives the shimmer.
                        ui.Skeleton(220, 18);
                        ui.Skeleton(300, 14);
                        ui.Skeleton(260, 14);
                    }
                    using (ui.Card())
                        if (ui.EmptyState("No documents yet", "Import a folder to start building your workspace.", "Import documents"))
                            notice = true;
                }
            }
        }
        else if (KitTab == 1)
        {
            using (ui.Card("Browse a workspace"))
            {
                ui.Breadcrumbs(["Workspace", "Projects", "Interface kit"], ref KitCrumb);
                ui.Segmented(["All files", "Recent", "Shared"], ref KitSegment);
                ui.Text($"View: {KitSegment + 1} · Chosen ancestor: {KitCrumb}").Color(T.Muted);
                ui.Divider();
                using (var disclosure = ui.Collapsible("Project details", ref KitDisclosure))
                    if (disclosure.IsOpen)
                    {
                        ui.Text("A reusable disclosure uses ordinary immediate children.").FillWidth();
                        ui.Text("Closing it unmounts its content and component-local state.").Color(T.Muted).FillWidth();
                    }
                int action = ui.MenuButton("Actions…", KitCommands);
                if (action >= 0) KitMenuResult = KitCommands[action].Label;
                ui.Text(KitMenuResult).Color(T.Muted);
                ui.Pagination(ref KitPage, 120);
                ui.Text($"Page {KitPage} of 120").Color(T.Muted);
            }
        }
        else if (KitTab == 2)
        {
            using (ui.Card("Account settings", "Visible labels, contextual help and ordinary application state."))
            {
                using (ui.Field("Receipt email", "Used only for receipts.", KitEmail.Length > 0 && !KitEmail.Contains('@') ? "Enter a valid email address." : null))
                    ui.TextInput("Kit receipt email", ref KitEmail).FillWidth();
                ui.SearchField("Search components", ref KitQuery);
                using (ui.Field("Copies", "Choose between 1 and 99.")) ui.NumberInput("Kit copies", ref KitCopies, 1, 99);
                using (ui.Field("Reminder", "24-hour time.")) ui.TimeInput("Kit reminder", ref KitTime);
                ui.ColorPicker("Accent", ref KitColor);
            }
        }
        else CornerStudy(ui);
    }
    private void CornerStudy(Ui ui)
    {
        using (ui.Card("Continuous corners", "The same radius, with a gentler transition into straight edges."))
        {
            ui.Slider("Corner smoothing", ref CornerAmount, 0, 1).Width(260);
            ui.Text($"Smoothing: {CornerAmount:0.00}").Color(T.Muted);
            foreach (string example in new[] { "Filled surface", "Border outline", "Clipped artwork" })
                using (ui.Scope(example))
                {
                    ui.Text(example).Weight(650);
                    using (ui.Row().FillWidth().Gap(24).Wrap())
                        for (int i = 0; i < 2; i++)
                            using (ui.Scope(i))
                            using (ui.Column().Gap(10).Shrink())
                            {
                                float smoothing = i == 0 ? 0 : (float)CornerAmount;
                                using (var surface = ui.Box().Width(300).Height(160).Radius(28).CornerSmoothing(smoothing)
                                    .AlignCenter().JustifyCenter())
                                {
                                    if (example == "Clipped artwork")
                                    { surface.Clip(); ui.Shader(LyricArtwork).Absolute(0, 0).FillWidth().FillHeight(); }
                                    else if (example == "Border outline") surface.Border(2, T.Accent);
                                    else surface.Background(T.Accent);
                                }
                                ui.Text(i == 0 ? "Circular · radius 28" : "Continuous · radius 28").Color(T.Muted);
                            }
                }
        }
    }

}
