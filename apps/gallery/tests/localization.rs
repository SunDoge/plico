use gallery::{
    Gallery, Page,
    i18n::{self, Language},
};
use plico::{Engine, Entity, Kind, Route, Router, ViewNode};
use plico_dev::App;

fn find(engine: &Engine, parent: Entity, label: &str) -> Entity {
    fn search(engine: &Engine, id: Entity, label: &str) -> Option<Entity> {
        let node = engine.node(id)?;
        if matches!(node.kind, Kind::Button | Kind::ButtonBase)
            && engine.accessible_name(id).as_deref() == Some(label)
        {
            return Some(id);
        }
        node.children
            .iter()
            .find_map(|id| search(engine, *id, label))
    }
    search(engine, parent, label).unwrap_or_else(|| panic!("missing button {label}"))
}
fn text(nodes: &[ViewNode], label: &str) -> bool {
    nodes
        .iter()
        .any(|n| n.text == label || text(&n.children, label))
}
fn frame(engine: &mut Engine, app: &mut Gallery) {
    engine.frame_declarations(|cx| app.show(cx)).unwrap();
}
#[test]
fn language_switch_preserves_route_count_and_widget_identity() {
    let mut app = Gallery {
        router: Router::new(Page::Controls),
        count: 12,
        ..Default::default()
    };
    let mut engine = Engine::default();
    frame(&mut engine, &mut app);
    let increment = find(&engine, engine.root(), "Increment");
    let chinese = find(&engine, engine.root(), "中文");
    engine.queue_click(chinese);
    frame(&mut engine, &mut app);
    frame(&mut engine, &mut app);
    assert_eq!(app.language, Language::Chinese);
    assert_eq!(app.router.current(), Page::Controls);
    assert_eq!(app.count, 12);
    assert_eq!(find(&engine, engine.root(), "增加"), increment);
    assert!(text(&engine.snapshot(), "控件"));
    engine.queue_click(increment);
    frame(&mut engine, &mut app);
    assert_eq!(app.count, 13);
    let english = find(&engine, engine.root(), "English");
    engine.queue_click(english);
    frame(&mut engine, &mut app);
    frame(&mut engine, &mut app);
    assert_eq!(find(&engine, engine.root(), "Increment"), increment);
    assert_eq!(app.count, 13);
}
#[test]
fn every_page_declares_in_both_languages_without_catalog_errors() {
    for language in [Language::English, Language::Chinese] {
        for &page in Page::ALL {
            // Images starts host IO in desktop builds; it has its own host tests.
            if page == Page::Images {
                continue;
            }
            let mut app = Gallery {
                language,
                router: Router::new(page),
                ..Default::default()
            };
            let mut engine = Engine::default();
            frame(&mut engine, &mut app);
            assert!(
                i18n::catalog(language).take_errors().is_empty(),
                "{language:?}/{page:?}"
            );
        }
    }
}
#[test]
fn scenario_language_round_trips_and_defaults_to_english() {
    let mut app: Gallery = serde_json::from_str(r#"{"language":"zh-CN","count":9}"#).unwrap();
    assert_eq!(app.language, Language::Chinese);
    let json = serde_json::to_string(&app).unwrap();
    app = serde_json::from_str(&json).unwrap();
    assert_eq!(app.count, 9);
    assert_eq!(app.language, Language::Chinese);
    assert_eq!(
        serde_json::from_str::<Gallery>("{}").unwrap().language,
        Language::English
    );
}
