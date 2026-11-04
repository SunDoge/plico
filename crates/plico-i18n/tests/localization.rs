#![cfg(feature = "embedded-data")]
use plico::{CalendarDate, CalendarState, Engine, Localizer, ModalState, WeekStart, widgets::*};
use plico_i18n::{Catalog, Error, FluentArgs, widgets};
use std::sync::Arc;

fn date() -> CalendarDate {
    CalendarDate::new(2026, 11, 3).unwrap()
}
fn en() -> Catalog {
    Catalog::new("en-US", [widgets::EN]).unwrap()
}
fn zh() -> Catalog {
    Catalog::new("zh-CN", [widgets::ZH_CN]).unwrap()
}

#[test]
fn fluent_variables_plural_rules_fallback_and_diagnostics() {
    let fallback = Arc::new(Catalog::new("en-US", ["hello = Hello, { $name }!\nitems = { $count ->\n [one] One item\n *[other] { $count } items\n}\nonly-english = Fallback\nbroken = { $missing }"]).unwrap());
    let translated = Catalog::new(
        "zh-CN",
        ["hello = 你好，{ $name }！\nitems = { $count } 项"],
    )
    .unwrap()
    .with_fallback(fallback.clone());
    let mut args = FluentArgs::new();
    args.set("name", "Plico");
    args.set("count", 1);
    assert!(
        translated
            .format("hello", Some(&args))
            .unwrap()
            .contains("Plico")
    );
    assert_eq!(fallback.format("items", Some(&args)).unwrap(), "One item");
    args.set("count", 2);
    assert!(fallback.format("items", Some(&args)).unwrap().contains("2"));
    assert!(
        translated
            .format("items", Some(&args))
            .unwrap()
            .contains("项")
    );
    assert_eq!(translated.format("only-english", None).unwrap(), "Fallback");
    assert!(matches!(
        translated.format("unknown", None),
        Err(Error::Missing { .. })
    ));
    assert!(matches!(
        translated.format("broken", None),
        Err(Error::Format { .. })
    ));
    assert_eq!(translated.text("broken", "Safe label"), "Safe label");
    assert_eq!(translated.take_errors().len(), 1);
    assert!(translated.take_errors().is_empty());
    assert!(Catalog::new("en", ["not a valid FTL resource"]).is_err());
    assert!(Catalog::new("en", ["a = one", "a = two"]).is_err());
    assert!(Catalog::new("not@a@locale", [""]).is_err());
}
#[test]
fn trimmed_icu_data_formats_dates_weekdays_and_exact_integers() {
    let en = en();
    let zh = zh();
    assert_eq!(en.number(1_234_567), "1,234,567");
    assert!(matches!(
        zh.text("dialog-close", "Close dialog"),
        std::borrow::Cow::Borrowed(_)
    ));
    assert_eq!(
        Catalog::new("en-US-u-nu-latn", [widgets::EN])
            .unwrap()
            .number(1234),
        "1,234"
    );
    assert_eq!(
        en.number(9_007_199_254_740_993_i64),
        "9,007,199,254,740,993"
    );
    assert_eq!(en.month_year(date()), "November 2026");
    assert!(zh.month_year(date()).contains("2026年11月"));
    assert_eq!(en.weekday(0), "Sun");
    assert_eq!(zh.weekday(0), "周日");
    assert!(zh.date(date()).contains("2026"));
    assert!(en.date(date()).contains("Nov"));
    assert!(Catalog::with_data("en", [""], b"not an ICU blob").is_err());
    assert!(include_bytes!("../data/icu4x.postcard").len() < 50_000);
}
#[test]
fn switching_catalog_updates_existing_nodes_without_changing_widget_identity() {
    let en = en();
    let zh = zh();
    let mut engine = Engine::default();
    let mut calendar = CalendarState::new(date());
    let draw = |engine: &mut Engine, localizer: &dyn Localizer, state: &mut CalendarState| {
        engine
            .frame_declarations(|cx| {
                cx.localized(localizer, |cx| {
                    let label = cx.tr("dialog-close", "Close dialog");
                    button(cx).text(label).key("action");
                    calendar_with(cx, state, WeekStart::Monday, Some(date()));
                })
            })
            .unwrap();
    };
    draw(&mut engine, &en, &mut calendar);
    let id = engine.find(engine.root(), "action").unwrap();
    let before = engine.node(id).unwrap().text.to_owned();
    draw(&mut engine, &zh, &mut calendar);
    assert_eq!(engine.find(engine.root(), "action"), Some(id));
    assert_ne!(engine.node(id).unwrap().text, before);
    assert!(format!("{:?}", engine.snapshot()).contains("关闭对话框"));
    assert_eq!(calendar.month(), date().first_of_month());
}
#[test]
fn nested_scopes_portals_and_unwinding_do_not_leak_locale() {
    let en = en();
    let zh = zh();
    let mut engine = Engine::default();
    let mut open = ModalState::default();
    open.open();
    engine
        .frame_declarations(|cx| {
            assert_eq!(cx.localizer().locale(), "en");
            cx.localized(&zh, |cx| {
                column(cx).children(|cx| {
                    assert_eq!(cx.localizer().locale(), "zh-CN");
                    cx.keyed("scope", |cx| assert_eq!(cx.localizer().locale(), "zh-CN"));
                    modal(cx, &mut open).title("Dialog").children(|cx| {
                        assert_eq!(cx.localizer().locale(), "zh-CN");
                        cx.localized(&en, |cx| assert_eq!(cx.localizer().locale(), "en-US"));
                        assert_eq!(cx.localizer().locale(), "zh-CN");
                    });
                });
            });
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cx.localized(&zh, |_| panic!("scope exit"))
            }));
            assert!(result.is_err());
            assert_eq!(cx.localizer().locale(), "en");
        })
        .unwrap();
    assert!(format!("{:?}", engine.snapshot()).contains("关闭对话框"));
}
