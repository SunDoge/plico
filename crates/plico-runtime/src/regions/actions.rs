//! One-shot native requests consumed by the declaration that observes them.
use super::*;

fn consume<D: Dependencies>(deps: D, callback: EventCallback<D>) {
    commands()
        .borrow_mut()
        .actions
        .push(EventSpec(Box::new(Event { deps, callback })));
}
/// Colocate an explicit focus request with cleanup of its application flag.
/// The callback runs after this declaration and normally uses `update_if(...; false)`
/// so consuming the command does not schedule another pass. If the input is not
/// mounted, no request or cleanup occurs and the flag remains pending.
pub trait FocusOnce {
    fn request_focus_once<D: Dependencies>(
        self,
        requested: bool,
        deps: D,
        callback: EventCallback<D>,
    ) -> Self;
}
impl FocusOnce for plico::Input<'_, '_, '_> {
    fn request_focus_once<D: Dependencies>(
        self,
        requested: bool,
        deps: D,
        callback: EventCallback<D>,
    ) -> Self {
        let input = self.request_focus(requested);
        if requested {
            consume(deps, callback);
        }
        input
    }
}
/// Reveal a virtual row once, leaving later wheel/trackpad scrolling untouched.
/// Cleanup runs only when the list declares this request.
pub trait RevealOnce {
    fn reveal_once<D: Dependencies>(
        self,
        index: Option<usize>,
        deps: D,
        callback: EventCallback<D>,
    ) -> Self;
}
impl RevealOnce for plico::VirtualList<'_, '_> {
    fn reveal_once<D: Dependencies>(
        self,
        index: Option<usize>,
        deps: D,
        callback: EventCallback<D>,
    ) -> Self {
        let list = self.reveal(index);
        if index.is_some() {
            consume(deps, callback);
        }
        list
    }
}
