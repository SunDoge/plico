using Plico;

internal static class ComponentKitChecks
{
    private static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
    internal static void Run()
    {
        int selected = 0, page = 1, crumb = -1, command = -1;
        bool chip = false, tag = true;
        using (var app = new Application(ui =>
        {
            using (ui.Column().Gap(8))
            {
                ui.Segmented(["First segment", "Second segment"], ref selected);
                ui.Breadcrumbs(["Root crumb", "Branch crumb", "Current crumb"], ref crumb);
                ui.Pagination(ref page, int.MaxValue);
                ui.Chip("Active chip", ref chip);
                if (tag && ui.Tag("Document tag")) tag = false;
                int result = ui.MenuButton("Commands", [new("copy", "Copy item"), new("delete", "Unavailable delete", true)]);
                if (result >= 0) command = result;
            }
        }))
        {
            app.Step(1200, 900); Check(app.NodeCount < 100, "Pagination grew with page count");
            app.Click("button", "Second segment"); app.Step(); Check(selected == 1, "Segment selection failed");
            app.Click("button", "Root crumb"); app.Step(); Check(crumb == 0, "Breadcrumb did not select ancestor");
            app.Click("button", "Next page"); app.Step(); Check(page == 2, "Pagination next failed");
            page = int.MaxValue; app.Invalidate(); app.Step(); Check(app.NodeCount < 100, "Final-page bounds overflowed");
            app.Click("button", "Previous page"); app.Step(); Check(page == int.MaxValue - 1, "Pagination previous failed");
            app.Click("button", "Active chip"); app.Step(); Check(chip, "Chip failed");
            app.Click("button", "Remove Document tag"); app.Step(); Check(!tag, "Tag removal failed");
            app.Click("button", "Commands"); app.Step();
            app.KeyPress("Escape"); app.Step();
            bool dismissed = false;
            try { app.Click("button", "Copy item"); } catch (InvalidOperationException) { dismissed = true; }
            Check(dismissed, "Menu did not dismiss on Escape");
            app.Click("button", "Commands"); app.Step();
            bool disabled = false;
            try { app.Click("button", "Unavailable delete"); } catch (InvalidOperationException) { disabled = true; }
            Check(disabled && command == -1, "Disabled menu action was activated");
            app.Click("button", "Copy item"); app.Step(); Check(command == 0, "Menu command failed");
        }
        bool expanded = true; int local = -1;
        using (var app = new Application(ui =>
        {
            using var section = ui.Collapsible("Details", ref expanded);
            if (section.IsOpen)
            {
                ref int count = ref ui.State(static () => 0);
                if (ui.Button("Increment disclosure").Clicked()) count++;
                local = count;
            }
        }))
        {
            app.Step(); app.Click("button", "Increment disclosure"); app.Step(); Check(local == 1, "Disclosure state failed");
            app.Click("button", "Details"); app.Step(); Check(!expanded, "Disclosure failed to close");
            app.Click("button", "Details"); app.Step(); Check(expanded && local == 0, "Disclosure did not unmount state");
        }
        string query = "", email = ""; uint color = 0x102030; double low = 20, high = 80;
        using (var app = new Application(ui =>
        {
            using (ui.Column().Gap(8))
            {
                using (ui.Field("Email", error: email.Contains('@') ? null : "Enter an email")) ui.TextInput("Form email", ref email);
                ui.SearchField("Find documents", ref query);
                ui.RangeSlider("Cost", ref low, ref high);
                ui.ColorPicker("Tint", ref color);
                ui.Skeleton(animated: false);
                ui.Avatar("e\u0301 日本語");
            }
        }))
        {
            app.Step(); app.SetText("Form email", "user@example.com"); app.Step(); Check(email == "user@example.com", "Form input failed");
            app.SetText("Find documents", "needle"); app.Step(); Check(query == "needle", "Search input failed");
            app.Click("button", "Clear Find documents"); app.Step(); Check(query == "", "Search clear failed");
            app.SetValue("Cost minimum", 35); app.Step(); Check(low == 35 && high == 80, "Range endpoints crossed");
            app.SetValue("Tint red", 240); app.Step(); Check(color == 0xf02030, "RGB channel update failed");
            ulong before = app.Stats.Callbacks; app.Step(); app.Step(); Check(app.Stats.Callbacks == before, "Static skeleton rebuilt C#");
        }
        bool open = false, notice = true; int action = -1, current = 1; string find = "fox";
        using (var app = new Application(ui =>
        {
            using (ui.Column())
            {
                ui.Notification("Saved", ref notice);
                ui.FindBar(ref find, 3, ref current);
                if (ui.Button("Show confirmation").Clicked()) open = true;
                int result = ui.AlertDialog(ref open, "Confirm action?", "This cannot be undone.", ["Keep item", "Remove item"]);
                if (result >= 0) action = result;
            }
        }))
        {
            app.Step(); app.Click("button", "Dismiss notification"); app.Step(); Check(!notice, "Notification did not dismiss");
            app.Click("button", "Previous match"); app.Step(); Check(current == 3, "Find previous did not wrap");
            app.Click("button", "Next match"); app.Step(); Check(current == 1, "Find next did not wrap");
            app.Click("button", "Show confirmation"); app.Step();
            app.KeyPress("Escape"); app.Step(); Check(!open && action == -1, "Alert dismissal became confirmation");
            app.Click("button", "Show confirmation"); app.Step();
            app.Click("button", "Remove item"); app.Step(); Check(!open && action == 1, "Alert result failed");
        }
        double copies = 2; TimeOnly reminder = new(15, 30, 45);
        using (var app = new Application(ui =>
        {
            using (ui.Column())
            {
                ui.NumberInput("Quantity", ref copies, -10, 10, .5);
                ui.TimeInput("Reminder", ref reminder);
            }
        }))
        {
            app.Step(); Check(reminder.Second == 45, "Time field lost seconds without an edit");
            app.SetText("Quantity", "-"); app.Step(); Check(copies == 2, "Incomplete numeric input corrupted typed value");
            app.SetText("Quantity", "3.5"); app.Step(); Check(copies == 3.5, "Numeric edit did not apply");
            app.Click("button", "Increase Quantity"); app.Step(); Check(copies == 4, "Numeric step failed");
            app.SetText("Quantity", "Infinity"); app.Step(); Check(copies == 4, "Nonfinite edit was accepted");
            copies = -5; app.Invalidate(); app.Step();
            app.Click("button", "Increase Quantity"); app.Step(); Check(copies == -4.5, "External numeric replacement failed");
            app.SetText("Reminder", "25:70"); app.Step(); Check(reminder == new TimeOnly(15, 30, 45), "Invalid time corrupted typed value");
            app.SetText("Reminder", "09:45"); app.Step(); Check(reminder == new TimeOnly(9, 45), "Valid time did not apply");
        }
        int formats = 0, sorted = -1;
        int[] rows = Enumerable.Range(0, 10000).ToArray();
        TableColumn<int>[] columns = [new("id", "Identifier", row => { formats++; return $"Row {row}"; })];
        using (var app = new Application(ui =>
        {
            int result = ui.Table(rows, columns, row => row.ToString(), height: 240);
            if (result >= 0) sorted = result;
        }))
        {
            app.Step(800, 600); Check(formats < 100 && app.NodeCount < 150, "Virtualized table declared hidden rows");
            app.Click("button", "Identifier"); app.Step(800, 600); Check(sorted == 0, "Table header event failed");
            app.Scroll(100, 150, -800); app.Step(800, 600);
            Check(formats < 300 && app.NodeCount < 150, "Table scroll traversed all rows");
        }
        Console.WriteLine("Component kit checks passed: bounded pagination, navigation, disabled commands, popup Escape, disclosure lifetime, forms, colors and confirmation");
    }
}
