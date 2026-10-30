//! NSH component host. Constructed on NSH's engine thread, so neither the
//! Plico application nor its `Engine` needs to be `Send`.

use crate::Adapter;
use nsh_core::element::{
    AnyElement, AnyValue, Children, Component, IntoElement, NoEvent, PropError, PropSpec, Props,
    Root, Scope, Slot, TsTypes, dynamic, signal, stored, text,
};
use plico::{DeclareView, Engine, Entity};
use std::rc::Rc;

#[cfg(target_os = "linux")]
pub use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
pub use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
pub use nsh_platform_windows as platform;

/// Make an NSH root for an ordinary Plico `DeclareView` factory.
pub fn root<V: DeclareView + 'static>(make: fn() -> V) -> Root {
    root_with_viewport(make, 720., 820.)
}

/// Make an NSH root and initialize Plico's logical viewport. This viewport is
/// declaration context only; NSH still owns actual window layout.
pub fn root_with_viewport<V: DeclareView + 'static>(
    make: fn() -> V,
    width: f32,
    height: f32,
) -> Root {
    Root::new::<PlicoComponent<V>>(move || Factory {
        make: Some(make),
        viewport: [width, height],
    })
}

struct Factory<V> {
    make: Option<fn() -> V>,
    viewport: [f32; 2],
}

impl<V> Clone for Factory<V> {
    fn clone(&self) -> Self {
        Self {
            make: self.make,
            viewport: self.viewport,
        }
    }
}

impl<V> PartialEq for Factory<V> {
    fn eq(&self, other: &Self) -> bool {
        let make_matches = match (self.make, other.make) {
            (Some(left), Some(right)) => std::ptr::fn_addr_eq(left, right),
            (None, None) => true,
            _ => false,
        };
        make_matches && self.viewport == other.viewport
    }
}

impl<V: 'static> Props for Factory<V> {
    fn spec(_types: &mut TsTypes) -> Vec<PropSpec> {
        Vec::new()
    }

    fn initial() -> Self {
        Self {
            make: None,
            viewport: [720., 820.],
        }
    }

    fn set(&mut self, name: &str, _value: Option<AnyValue>) -> Result<bool, PropError> {
        Err(PropError(format!("unknown prop `{name}`")))
    }

    fn set_slot(&mut self, _name: &str, _slot: Option<Slot>) -> bool {
        false
    }

    fn set_children(&mut self, _children: Children) -> bool {
        false
    }
}

struct PlicoComponent<V>(std::marker::PhantomData<fn() -> V>);

impl<V: DeclareView + 'static> Component for PlicoComponent<V> {
    type Props = Factory<V>;
    type Event = NoEvent;

    fn view(props: Self::Props, _cx: Scope<Self::Event>) -> impl IntoElement {
        let make = props.make.expect("Plico root factory");
        let host = stored(Host::new(make, props.viewport));
        let revision = signal(0u64);
        dynamic(move || {
            revision.get();
            let adapter = host.with(|host| host.adapter.clone());
            let route = Rc::new(move |entity: Entity| {
                adapter.send_click(entity);
                revision.update(|revision| *revision += 1);
            });
            host.with(|host| host.frame(route))
        })
    }
}

struct Host<V> {
    engine: Engine,
    app: V,
    adapter: Adapter,
    viewport_error: Option<plico::FrameError>,
}

impl<V: DeclareView> Host<V> {
    fn new(make: fn() -> V, viewport: [f32; 2]) -> Self {
        let mut engine = Engine::default();
        let viewport_error = engine.set_viewport(viewport[0], viewport[1]).err();
        Self {
            engine,
            app: make(),
            adapter: Adapter::default(),
            viewport_error,
        }
    }

    fn frame(&mut self, route: Rc<dyn Fn(Entity)>) -> AnyElement {
        if let Some(error) = self.viewport_error {
            return error_element(format!("invalid Plico viewport: {error}"));
        }
        self.adapter.dispatch(&mut self.engine);
        if let Err(error) = self.engine.declare_until_settled(&mut self.app) {
            return error_element(format!("Plico declaration failed: {error}"));
        }
        match self.adapter.project_with(&self.engine, route) {
            Ok(element) => element,
            Err(error) => error_element(format!("NSH projection failed: {error}")),
        }
    }
}

fn error_element(message: String) -> AnyElement {
    text(message).into_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsh_core::{
        ApproxTextEngine, Core, EventKind, EventPayload, MountItem, Size, geometry::px,
    };
    use plico::prelude::*;

    #[derive(Default)]
    struct Toggle {
        disabled: bool,
    }

    impl DeclareView for Toggle {
        fn theme(&self) -> plico::Theme {
            plico::Theme::default()
        }

        #[component]
        #[rustfmt::skip]
        fn show(&mut self, cx: &mut UiContext) {
            if button(cx).text(if self.disabled { "Enable increment" } else { "Disable increment" }).clicked() {
                self.disabled = !self.disabled;
            }
        }
    }

    #[test]
    fn nsh_reconciles_dynamic_button_text() {
        let mut core = Core::new(ApproxTextEngine, Size::new(px(480.0), px(360.0)));
        core.mount_root(&root_with_viewport(Toggle::default, 480.0, 360.0));
        let first = core.commit().expect("initial NSH commit");
        let label = first
            .items
            .iter()
            .find_map(|item| match item {
                MountItem::Text {
                    id, text: content, ..
                } if content.text == "Disable increment" => Some(*id),
                _ => None,
            })
            .expect("mounted Disable increment text");
        let button = first
            .items
            .iter()
            .find_map(|item| match item {
                MountItem::ViewProps { id, props }
                    if props.accessibility.label.as_deref() == Some("Disable increment") =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .expect("projected Disable increment button");
        assert!(core.component_event(button, EventKind::Press, &EventPayload::None));
        let second = core.commit().expect("NSH commit after button press");
        let updated = second.items.iter().find_map(|item| match item {
            MountItem::Text {
                id, text: content, ..
            } if content.text == "Enable increment" => Some(*id),
            _ => None,
        });
        assert_eq!(
            updated,
            Some(label),
            "updated NSH mount batch: {:?}",
            second.items
        );
    }
}
