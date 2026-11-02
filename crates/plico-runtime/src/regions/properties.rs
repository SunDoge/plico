//! Explicit property subscriptions reuse the region scheduler and owner lifetime.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum PropertyKind {
    Text,
    Button,
}
/// Restricted property editor. No tree access and no persistent widget borrow.
/// Each callback starts from the ordinary builder's text and attributes. Omitting a
/// property restores that baseline, so local and full declarations agree.
#[derive(Default)]
pub struct TextProperties {
    text: Option<String>,
    attributes: plico::ExperimentTextStyle,
}
impl TextProperties {
    pub fn text(&mut self, value: impl Into<String>) -> &mut Self {
        self.text = Some(value.into());
        self
    }
    plico::macro_support::text_style_methods!(borrowed(attributes));
}
/// Button state that can change without redeclaring the control's children.
pub struct ButtonProperties {
    attributes: plico::ExperimentButtonStyle,
}
impl ButtonProperties {
    pub fn background(&mut self, value: plico::Color) -> &mut Self {
        self.attributes.background = Some(value);
        self
    }
    pub fn disabled(&mut self, value: bool) -> &mut Self {
        self.attributes.disabled = value;
        self
    }
    pub fn selected(&mut self, value: bool) -> &mut Self {
        self.attributes.selected = Some(value);
        self
    }
}
struct TextBinding<D: Dependencies> {
    deps: D,
    callback: D::PropertyCallback<TextProperties>,
    baseline_text: String,
    baseline_attributes: plico::ExperimentTextStyle,
}
impl<D: Dependencies> Render for TextBinding<D> {
    fn property(&self) -> Option<PropertyKind> {
        Some(PropertyKind::Text)
    }
    fn run(&self, app: &Regions, cx: &mut UiContext<'_>) {
        let mut values = TextProperties {
            attributes: self.baseline_attributes,
            ..TextProperties::default()
        };
        self.deps.properties(self.callback, app, &mut values);
        cx.experiment_text_properties(
            Some(values.text.as_deref().unwrap_or(&self.baseline_text)),
            values.attributes,
        );
    }
}
struct ButtonBinding<D: Dependencies> {
    deps: D,
    callback: D::PropertyCallback<ButtonProperties>,
    baseline_attributes: plico::ExperimentButtonStyle,
}
impl<D: Dependencies> Render for ButtonBinding<D> {
    fn property(&self) -> Option<PropertyKind> {
        Some(PropertyKind::Button)
    }
    fn run(&self, app: &Regions, cx: &mut UiContext<'_>) {
        let mut values = ButtonProperties {
            attributes: self.baseline_attributes,
        };
        self.deps.properties(self.callback, app, &mut values);
        cx.experiment_button_properties(values.attributes);
    }
}
/// Bind a builder to explicit state IDs and a non-capturing callback.
/// Registration finishes text leaves; buttons return their ordinary input Response,
/// so event handlers can remain on the same chain. A parent declaration replaces
/// the binding and reapplies its values; unmount removes it with its owning region.
///
/// ```
/// use plico::prelude::*;
/// use plico_runtime::regions::{Reactive, Regions};
/// let app = Regions::new(|app| {
///     let count = app.state(|| 0_i32);
///     app.root((count,), |cx, (count,)| {
///         text_empty(cx).font_size(24).rx(count.handle(), |node, count| {
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
///         text_empty(cx).rx((), |node, ()| { node.text(hidden.clone()); });
///     });
/// });
/// ```
/// Structure is declared by the ordinary builder, outside the property callback.
/// ```compile_fail
/// use plico::prelude::*;
/// use plico_runtime::regions::{Reactive, Regions};
/// let app = Regions::new(|app| {
///     app.root((), |cx, ()| {
///         text_empty(cx).rx((), |node, ()| {
///             node.children(|cx| { text(cx, "new child"); });
///         });
///     });
/// });
/// ```
pub trait Reactive {
    type Properties;
    type Response;
    /// Bind one state directly, or pass a tuple to receive a tuple of accessors.
    fn rx<D: Dependencies>(
        self,
        deps: D,
        callback: D::PropertyCallback<Self::Properties>,
    ) -> Self::Response;

    #[deprecated(since = "0.1.9", note = "use rx instead")]
    fn reactive<D: Dependencies>(
        self,
        deps: D,
        callback: D::PropertyCallback<Self::Properties>,
    ) -> Self::Response
    where
        Self: Sized,
    {
        self.rx(deps, callback)
    }
}
impl Reactive for plico::Text<'_, '_, '_> {
    type Properties = TextProperties;
    type Response = ();
    fn rx<D: Dependencies>(self, deps: D, callback: D::PropertyCallback<TextProperties>) {
        let (context, baseline_text, baseline_attributes) = self.experiment_property_target();
        commands().borrow_mut().children.push(Child {
            key: ChildKey {
                target: context.parent,
                component: false,
            },
            context,
            path: vec![],
            spec: Spec {
                deps: deps.bindings(),
                subscriptions: None,
                render: Box::new(TextBinding {
                    deps,
                    callback,
                    baseline_text,
                    baseline_attributes,
                }),
                fill: false,
                gap: None,
                scene_cache: true,
            },
        });
    }
}
impl Reactive for plico::Button<'_, '_, '_> {
    type Properties = ButtonProperties;
    type Response = Response;
    fn rx<D: Dependencies>(
        self,
        deps: D,
        callback: D::PropertyCallback<ButtonProperties>,
    ) -> Response {
        let (context, baseline_attributes, response) = self.experiment_property_target();
        commands().borrow_mut().children.push(Child {
            key: ChildKey {
                target: context.parent,
                component: false,
            },
            context,
            path: vec![],
            spec: Spec {
                deps: deps.bindings(),
                subscriptions: None,
                render: Box::new(ButtonBinding {
                    deps,
                    callback,
                    baseline_attributes,
                }),
                fill: false,
                gap: None,
                scene_cache: true,
            },
        });
        response
    }
}
