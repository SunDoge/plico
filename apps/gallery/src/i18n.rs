//! The gallery embeds only its own message files and the en/zh ICU4X subset.
use plico::prelude::*;
use plico::{CalendarDate, CalendarState, WeekStart};
use plico_i18n::{Catalog, FluentArgs};
use std::sync::{Arc, OnceLock};

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-CN", alias = "zh")]
    Chinese,
}
pub fn catalog(language: Language) -> &'static Catalog {
    static EN: OnceLock<Arc<Catalog>> = OnceLock::new();
    static ZH: OnceLock<Catalog> = OnceLock::new();
    let english = EN.get_or_init(|| {
        Arc::new(
            Catalog::new(
                "en-US",
                [plico_i18n::widgets::EN, include_str!("../locales/en.ftl")],
            )
            .expect("validated English catalog"),
        )
    });
    match language {
        Language::English => english,
        Language::Chinese => ZH.get_or_init(|| {
            Catalog::new(
                "zh-CN",
                [
                    plico_i18n::widgets::ZH_CN,
                    include_str!("../locales/zh-CN.ftl"),
                ],
            )
            .expect("validated Chinese catalog")
            .with_fallback(english.clone())
        }),
    }
}

#[component]
#[rustfmt::skip]
pub fn page(cx: &mut UiContext, language: Language) {
    let l = cx.localizer();
    crate::heading(cx, &l.text("i18n-title", "Localization"),
        &l.text("i18n-description", "Messages, plurals, numbers and dates follow the selected language."));
    let count = cx.state(|| 1_i64);
    let nested = cx.state(|| CalendarState::new(CalendarDate::new(2026, 11, 1).unwrap()));
    let catalog = catalog(language);
    column(cx).gap(16).height(0.).grow(1.).scroll_y(true) + |cx| {
        column(cx).padding(24).gap(16).radius(12.).background(Color::Panel).border(1., Color::Border) + |cx| {
            let mut args = FluentArgs::new();
            args.set("name", "Plico");
            text(cx, catalog.format("i18n-greeting", Some(&args)).expect("valid greeting").into_owned()).font_size(24).weight(650);
            args.set("count", count.get());
            text(cx, catalog.format("i18n-count", Some(&args)).expect("valid plural").into_owned()).font_size(17);
            row(cx).gap(8).wrap(true) + |cx| {
                if button(cx).text(l.text("i18n-add", "Add one")).primary().clicked() {
                    count.update(|value| *value += 1);
                }
                if button(cx).text(l.text("reset", "Reset")).ghost().clicked() {
                    count.set(0);
                }
            };
            separator(cx);
            text(cx, l.text("i18n-number", "Formatted number")).color(Color::Muted);
            text(cx, catalog.number(1_234_567)).font_size(22).weight(600);
            text(cx, l.text("i18n-date", "Formatted date")).color(Color::Muted);
            text(cx, catalog.date(CalendarDate::new(2026, 11, 3).unwrap())).font_size(22).weight(600);
        };
        column(cx).padding(24).gap(12).radius(12.).background(Color::Panel).border(1., Color::Border) + |cx| {
            text(cx, l.text("i18n-scope", "A nested English scope")).font_size(17).weight(600);
            let mut state = nested.get();
            cx.localized(self::catalog(Language::English), |cx| {
                calendar_with(cx, &mut state, WeekStart::Sunday, Some(CalendarDate::new(2026, 11, 3).unwrap()));
            });
            nested.set(state);
            text(cx, l
                .text("i18n-scope-description", "This calendar stays English; the surrounding page follows your selection."))
                .color(Color::Muted);
        };
    };
}

pub fn count_message(language: Language, id: &str, count: i64) -> String {
    let mut args = FluentArgs::new();
    args.set("count", count);
    catalog(language)
        .format(id, Some(&args))
        .expect("validated count message")
        .into_owned()
}
