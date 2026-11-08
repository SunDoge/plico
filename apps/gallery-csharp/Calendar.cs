using Plico;
using System.Globalization;

namespace Gallery;

internal sealed partial class GalleryModel
{
    internal DateOnly Month = new(2026, 11, 1);
    internal DateOnly SelectedDate = new(2026, 11, 7);
    private bool calendarPopup;
    private DateOnly popupMonth = new(2026, 11, 1);
    internal string Note = "Review the gallery";
    private void Calendar(Ui ui)
    {
        using ((ui.Width >= 1050 ? ui.Row() : ui.Column()).FillWidth().Gap(24).AlignStart())
        {
            using (ui.Column().Width(380).Shrink().Padding(24).Gap(16).Border(1, T.Border).Radius(12))
            {
                using (ui.Row().FillWidth().AlignCenter().SpaceBetween())
                {
                    if (ui.Button("‹").AriaLabel("Previous month").Ghost().Clicked()) Month = Month.AddMonths(-1);
                    ui.Text(Month.ToString(Chinese ? "yyyy 年 M 月" : "MMMM yyyy", CultureInfo.InvariantCulture)).FontSize(17).Weight(650);
                    if (ui.Button("›").AriaLabel("Next month").Ghost().Clicked()) Month = Month.AddMonths(1);
                }
                using (ui.Row().Gap(4))
                    foreach (string day in Chinese ? new[] { "一", "二", "三", "四", "五", "六", "日" } : new[] { "Mo", "Tu", "We", "Th", "Fr", "Sa", "Su" })
                        using (ui.Scope(day)) ui.Text(day).Width(42).Color(T.Muted).Padding(8);
                int leading = ((int)Month.DayOfWeek + 6) % 7;
                for (int week = 0; week < 6; week++)
                {
                    using (ui.Scope(week))
                    using (ui.Row().Gap(4))
                        for (int weekday = 0; weekday < 7; weekday++)
                        {
                            int offset = week * 7 + weekday - leading;
                            var date = Month.AddDays(offset);
                            using (ui.Scope(date.DayNumber))
                            {
                                if (date.Month != Month.Month)
                                {
                                    using (ui.Column().Width(42).Height(38)) { }
                                }
                                else
                                {
                                    var button = ui.Button(date.Day.ToString(), key: date.ToString("yyyy-MM-dd")).Width(42).Height(38).Padding(0);
                                    if (date == SelectedDate) button = button.Primary(); else button = button.Ghost();
                                    if (button.Clicked()) SelectedDate = date;
                                }
                            }
                        }
                }
                Rule(ui);
                if (ui.Button("Go to November 2026").Ghost().FillWidth().Clicked()) Month = new(2026, 11, 1);
            }
            using (ui.Column().Width(0).Grow().MinWidth(240).Gap(16))
            using (Card(ui, Tr("Selected date", "选择的日期")))
            {
                ui.Text(SelectedDate.ToString("dddd", CultureInfo.InvariantCulture)).Color(T.Muted);
                ui.Text(SelectedDate.ToString("dd MMM yyyy", CultureInfo.InvariantCulture)).FontSize(28).Weight(650);
                ui.TextInput("Date note", ref Note).FillWidth();
                ui.DatePicker("Choose a date", ref SelectedDate, ref calendarPopup, ref popupMonth, CultureInfo.GetCultureInfo(Chinese ? "zh-CN" : "en-GB"));
                Hint(ui, "Month navigation and selection are independent. Your note stays when you change pages.");
            }
        }
    }
}
