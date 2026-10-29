//! Navigation state and mounted page ownership are separate concerns.
use super::*;

struct Page<R: Copy + Eq + 'static, D: Dependencies> {
    route: R,
    captures: D,
    render: D::RouteCallback<R>,
}
impl<R: Copy + Eq + 'static, D: Dependencies> Render for Page<R, D> {
    fn run(&self, app: &Regions, cx: &mut UiContext<'_>) {
        self.captures.route(self.render, app, cx, self.route);
    }
}
struct Outlet<R: Copy + Eq + 'static, D: Dependencies + Clone> {
    router: State<plico::Router<R>>,
    captures: D,
    identity: fn(R) -> Key,
    render: D::RouteCallback<R>,
    fill: bool,
}
impl<R: Copy + Eq + 'static, D: Dependencies + Clone> Render for Outlet<R, D> {
    fn run(&self, app: &Regions, cx: &mut UiContext<'_>) {
        let route = app.access(self.router).read().current();
        child_with_key(
            cx,
            (self.identity)(route),
            true,
            Spec {
                deps: self.captures.bindings(),
                subscriptions: None,
                render: Box::new(Page {
                    route,
                    captures: self.captures.clone(),
                    render: self.render,
                }),
                fill: self.fill,
            },
        );
    }
}

/// Mount one component owned by the current route's stable `Route::id()`.
/// Only the router is subscribed; `captures` supplies explicit State IDs to the
/// page, whose own watches/property bindings determine its subscriptions.
///
/// Same key preserves component state. Changing key unmounts the old page and
/// invalidates its handles and host result senders. History does not keep pages
/// alive: back/forward mounts a fresh instance of a previously removed page.
/// Callbacks can run again on navigation notifications or parent declarations;
/// stable component ownership does not promise one-time execution of the body.
///
/// ```
/// use plico::{Router, prelude::*};
/// use plico_runtime::regions::{Regions, WatchUi, outlet};
/// #[derive(Clone, Copy, PartialEq, Eq, plico::Route)]
/// enum Page { Home, Settings }
/// let app = Regions::new(|app| {
///     let router = app.state(|| Router::new(Page::Home));
///     app.root((router,), |cx, (router,)| {
///         outlet(cx, router.handle(), (), |cx, page, ()| {
///             let visits = cx.local_state(|| 0_i32);
///             text(cx, page.label());
///         });
///     });
/// });
/// ```
///
/// ```compile_fail
/// use plico::{Router, prelude::*};
/// use plico_runtime::regions::{Regions, outlet};
/// #[derive(Clone, Copy, PartialEq, Eq, plico::Route)]
/// enum Page { Home }
/// let app = Regions::new(|app| {
///     let router = app.state(|| Router::new(Page::Home));
///     app.root((router,), |cx, (router,)| {
///         let hidden = String::from("captured outside the State ID tuple");
///         outlet(cx, router.handle(), (), |cx, _, ()| { text(cx, &hidden); });
///     });
/// });
/// ```
#[track_caller]
pub fn outlet<R: plico::Route, D: Dependencies + Clone>(
    cx: &mut UiContext<'_>,
    router: State<plico::Router<R>>,
    captures: D,
    render: D::RouteCallback<R>,
) {
    outlet_keyed_impl(
        cx,
        router,
        captures,
        |route| Key::Static(route.id()),
        render,
        false,
    );
}

/// Route outlet whose watch and page boundaries fill the remaining flex space.
#[track_caller]
pub fn outlet_fill<R: plico::Route, D: Dependencies + Clone>(
    cx: &mut UiContext<'_>,
    router: State<plico::Router<R>>,
    captures: D,
    render: D::RouteCallback<R>,
) {
    outlet_keyed_impl(
        cx,
        router,
        captures,
        |route| Key::Static(route.id()),
        render,
        true,
    );
}

/// Parameterized route variant: explicitly choose component identity. Different
/// keys remount; the same key keeps local state while supplying the new route
/// value to the callback. Keys must describe the intended page instance.
/// Same-key routes must use compatible component-local state slots.
#[track_caller]
pub fn outlet_keyed<R: Copy + Eq + 'static, D: Dependencies + Clone>(
    cx: &mut UiContext<'_>,
    router: State<plico::Router<R>>,
    captures: D,
    identity: fn(R) -> Key,
    render: D::RouteCallback<R>,
) {
    outlet_keyed_impl(cx, router, captures, identity, render, false);
}

#[track_caller]
fn outlet_keyed_impl<R: Copy + Eq + 'static, D: Dependencies + Clone>(
    cx: &mut UiContext<'_>,
    router: State<plico::Router<R>>,
    captures: D,
    identity: fn(R) -> Key,
    render: D::RouteCallback<R>,
    fill: bool,
) {
    let site = Location::caller();
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (site.file(), site.line(), site.column()).hash(&mut hash);
    let binding = (router.owner, router.component, router.index);
    let mut deps = vec![binding];
    deps.extend(captures.bindings());
    child(
        cx,
        hash.finish(),
        false,
        Spec {
            deps,
            subscriptions: Some(vec![binding]),
            render: Box::new(Outlet {
                router,
                captures,
                identity,
                render,
                fill,
            }),
            fill,
        },
    );
}
