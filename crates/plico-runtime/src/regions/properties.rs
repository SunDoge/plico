//! Explicit property subscriptions reuse the region scheduler and owner lifetime.
use super::*;

/// Restricted property editor. No tree access and no persistent widget borrow.
/// Each callback starts from the ordinary builder's text and color. Omitting a
/// property restores that baseline, so local and full declarations agree.
#[derive(Default)]
pub struct TextProperties {
    text: Option<String>,
    color: Option<plico::Color>,
}
impl TextProperties {
    pub fn text(&mut self, value: impl Into<String>) -> &mut Self {
        self.text = Some(value.into());
        self
    }
    pub fn color(&mut self, value: plico::Color) -> &mut Self {
        self.color = Some(value);
        self
    }
}

struct Properties<D: Dependencies> {
    deps: D,
    callback: D::PropertyCallback,
    baseline_text: String,
    baseline_color: plico::Color,
}
impl<D: Dependencies> Render for Properties<D> {
    fn property(&self) -> bool {
        true
    }
    fn run(&self, app: &Regions, cx: &mut UiContext<'_>) {
        let mut values = TextProperties::default();
        self.deps.properties(self.callback, app, &mut values);
        cx.experiment_text_properties(
            Some(values.text.as_deref().unwrap_or(&self.baseline_text)),
            Some(values.color.unwrap_or(self.baseline_color)),
        );
    }
}

/// Bind an existing text leaf to explicit state IDs and a non-capturing callback.
/// Registration ends the builder. A parent declaration replaces the binding
/// and reapplies its values; unmount removes it together with its owning region.
///
/// ```
/// use plico::prelude::*;
/// use plico_runtime::regions::{Reactive, Regions};
/// let app = Regions::new(|app| {
///     let count = app.state(|| 0_i32);
///     app.root((count,), |cx, (count,)| {
///         text(cx, "").font_size(24).reactive((count.handle(),), |node, (count,)| {
///             node.text(format!("Count: {}", *count.read()));
///         });
///     });
/// });
/// ```
///
/// Captures must be explicit state handles, not a retained closure environment.
/// ```compile_fail
/// use plico::prelude::*;
/// use plico_runtime::regions::{Reactive, Regions};
/// let app = Regions::new(|app| {
///     app.root((), |cx, ()| {
///         let hidden = String::from("capture");
///         text(cx, "").reactive((), |node, ()| { node.text(hidden.clone()); });
///     });
/// });
/// ```
pub trait Reactive {
    fn reactive<D: Dependencies>(self, deps: D, callback: D::PropertyCallback);
}
impl Reactive for plico::Text<'_, '_, '_> {
    fn reactive<D: Dependencies>(self, deps: D, callback: D::PropertyCallback) {
        let (context, baseline_text, baseline_color) = self.experiment_property_target();
        commands().borrow_mut().children.push(Child {
            key: ChildKey {
                parent: context.parent,
                key: 0,
                component: false,
            },
            context,
            path: vec![],
            spec: Spec {
                deps: deps.bindings(),
                render: Box::new(Properties {
                    deps,
                    callback,
                    baseline_text,
                    baseline_color,
                }),
            },
        });
    }
}
