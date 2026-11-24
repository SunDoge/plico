//! AppKit menu tracking at a point in the owning view, as in MyGO's Darwin host.
use super::*;
use crate::ContextMenuItem;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
};
use objc2_app_kit::{NSMenu, NSMenuItem, NSView};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSString};
use std::cell::Cell;
use winit::window::Window;

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Cell<Option<usize>>]
    #[name = "PlicoMenuActionTarget"]
    struct ActionTarget;
    unsafe impl NSObjectProtocol for ActionTarget {}
    impl ActionTarget {
        #[unsafe(method(plicoPick:))]
        fn pick(&self, sender: &NSMenuItem) { self.ivars().set(Some(sender.tag() as usize)); }
    }
);

fn build(
    mtm: MainThreadMarker,
    entries: &[ContextMenuItem],
    target: &ActionTarget,
    ids: &mut Vec<String>,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for entry in entries {
        match entry {
            ContextMenuItem::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
            ContextMenuItem::Item {
                id,
                label,
                enabled,
                checked,
            } => {
                // The target lives throughout synchronous tracking and implements this selector.
                let item = unsafe {
                    menu.addItemWithTitle_action_keyEquivalent(
                        &NSString::from_str(label),
                        Some(sel!(plicoPick:)),
                        &NSString::new(),
                    )
                };
                unsafe {
                    item.setTarget(Some(target));
                }
                item.setTag(ids.len() as isize);
                ids.push(id.clone());
                item.setEnabled(*enabled);
                item.setState(isize::from(*checked));
            }
            ContextMenuItem::Submenu { label, items } => {
                let item = unsafe {
                    menu.addItemWithTitle_action_keyEquivalent(
                        &NSString::from_str(label),
                        None,
                        &NSString::new(),
                    )
                };
                item.setSubmenu(Some(&build(mtm, items, target, ids)));
            }
        }
    }
    menu
}

pub(super) fn show(
    window: &Arc<dyn Window>,
    items: &[ContextMenuItem],
    x: f64,
    y: f64,
) -> Result<Option<String>, String> {
    let mtm = MainThreadMarker::new().ok_or("menu requires AppKit main thread")?;
    let RawWindowHandle::AppKit(handle) =
        window.window_handle().map_err(|e| e.to_string())?.as_raw()
    else {
        return Err("expected an AppKit view".into());
    };
    // The live window owns its view throughout synchronous menu tracking.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let target = ActionTarget::alloc(mtm).set_ivars(Cell::new(None));
    let target: Retained<ActionTarget> = unsafe { msg_send![super(target), init] };
    let mut ids = Vec::new();
    let menu = build(mtm, items, &target, &mut ids);
    let point = NSPoint::new(
        x,
        if view.isFlipped() {
            y
        } else {
            view.bounds().size.height - y
        },
    );
    menu.popUpMenuPositioningItem_atLocation_inView(None, point, Some(view));
    Ok(target.ivars().get().map(|i| ids.swap_remove(i)))
}
