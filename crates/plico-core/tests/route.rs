use plico_core::prelude::*;

static HOME: SvgAsset = SvgAsset::new("<svg/>");
#[derive(Clone, Copy, Debug, PartialEq, Eq, Route)]
enum Page {
    #[route(id = "home", label = "Home page", icon = HOME)]
    Home = 7,
    Settings = 42,
    #[allow(non_camel_case_types)] // Exercise a raw keyword identifier.
    r#type,
}
#[derive(Clone, Copy, Route)]
enum RenamedPage {
    #[route(id = "home")]
    Dashboard,
}
// Metadata itself does not require Copy, Eq, Default or serde.
#[derive(Route)]
enum Minimal {
    One,
}
const LABEL: &str = Page::Home.label();
const ICON: Option<&SvgAsset> = Page::Home.icon();

#[test]
fn ordered_metadata_and_exact_lookup() {
    assert_eq!(Page::ALL, &[Page::Home, Page::Settings, Page::r#type]);
    assert_eq!(LABEL, "Home page");
    assert!(std::ptr::eq(ICON.unwrap(), &HOME));
    assert_eq!(Page::Settings.label(), "Settings");
    assert!(Page::Settings.icon().is_none());
    assert_eq!(Page::r#type.id(), "type");
    for &page in Page::ALL {
        assert_eq!(Page::from_id(page.id()), Some(page));
    }
    assert_eq!(Page::from_id("unknown"), None);
    assert_eq!(Page::from_id("Home"), None);
    assert_eq!(Page::from_id(" home"), None);
    assert_eq!(RenamedPage::Dashboard.id(), Page::Home.id());
    assert_eq!(RenamedPage::ALL.len(), 1);
    assert_eq!(Minimal::ALL.len(), 1);
    assert_eq!(Minimal::One.label(), "One");
}
#[test]
fn router_remains_independent_of_metadata() {
    let mut router = Router::new(Page::Home);
    router.go(Page::from_id("Settings").unwrap());
    assert!(router.show(Page::Settings));
    assert_eq!(router.back(), Page::Home);
    assert_eq!(router.forward(), Page::Settings);
}
