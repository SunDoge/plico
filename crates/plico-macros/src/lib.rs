//! Compile-time declaration sites. No file/line/column participates in identity.
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{ToTokens, quote};
use syn::{
    Expr, FnArg, Ident, ItemFn, Pat, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    visit_mut::{self, VisitMut},
};

fn runtime() -> Tokens {
    match proc_macro_crate::crate_name("plico") {
        Ok(proc_macro_crate::FoundCrate::Name(name)) => {
            let name = Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#name)
        }
        // The core also has examples and integration tests, where `crate`
        // refers to the test binary rather than the library itself.
        _ => quote!(::plico),
    }
}

struct FormattedText {
    context: Expr,
    format: Tokens,
}
impl Parse for FormattedText {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let context = input.parse()?;
        input.parse::<Token![,]>()?;
        let format: Tokens = input.parse()?;
        if format.is_empty() {
            return Err(input.error("expected a format string after the context"));
        }
        Ok(Self { context, format })
    }
}
fn formatted_call(input: FormattedText) -> Expr {
    let FormattedText { context, format } = input;
    let runtime = runtime();
    parse_quote!(#runtime::widgets::text_fmt(#context, ::core::format_args!(#format)))
}
struct Expand {
    ui: Ident,
    namespace: Tokens,
    runtime: Tokens,
    next: u32,
    builder_root: bool,
}
impl Expand {
    fn site(&mut self) -> Tokens {
        let n = self.next;
        self.next += 1;
        let namespace = &self.namespace;
        let runtime = &self.runtime;
        quote!({ const SITE: u64 = #runtime::macro_support::site_id(#namespace, #n); SITE })
    }
    fn is_ui(&self, expr: &Expr) -> bool {
        matches!(expr,Expr::Path(path) if path.path.is_ident(&self.ui))
    }
    fn formatted_text(&mut self, mac: &syn::Macro) -> Expr {
        match syn::parse2::<FormattedText>(mac.tokens.clone()) {
            Ok(input) => {
                let mut call = formatted_call(input);
                self.visit_expr_mut(&mut call);
                let path = &mac.path;
                parse_quote!(#path! { @__plico_expanded #call })
            }
            Err(error) => {
                let error = error.into_compile_error();
                parse_quote!({ #error })
            }
        }
    }
}
impl VisitMut for Expand {
    fn visit_stmt_mut(&mut self, stmt: &mut syn::Stmt) {
        if let syn::Stmt::Macro(m) = stmt
            && m.mac
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "text")
        {
            *stmt = syn::Stmt::Expr(self.formatted_text(&m.mac), m.semi_token);
            return;
        }
        visit_mut::visit_stmt_mut(self, stmt);
        // Rust warns about discarded non-unit arithmetic results. A standalone
        // children declaration deliberately discards its response, just like
        // children(...); value/tail positions keep their real return type.
        if let syn::Stmt::Expr(Expr::Binary(binary), Some(_)) = stmt
            && matches!(binary.op, syn::BinOp::Add(_))
            && matches!(&*binary.right, Expr::Closure(_))
        {
            let expr = binary.clone();
            *stmt = parse_quote!(let _ = #expr;);
        }
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        // operator<closure> is native Rust; only identity generation needs to
        // know the child context binding, exactly as for the children method.
        if let Expr::Binary(binary) = expr
            && matches!(binary.op, syn::BinOp::Add(_))
            && let Expr::Closure(closure) = &mut *binary.right
            && closure.inputs.len() == 1
            && let Some(Pat::Ident(binding)) = closure.inputs.first()
        {
            let previous = std::mem::replace(&mut self.builder_root, true);
            self.visit_expr_mut(&mut binary.left);
            self.builder_root = previous;
            let previous = std::mem::replace(&mut self.ui, binding.ident.clone());
            self.visit_expr_mut(&mut closure.body);
            self.ui = previous;
            return;
        }
        // A children receiver is a borrowing builder factory. Visit it in builder
        // position, then analyze its closure using the actual context binding.
        if let Expr::MethodCall(call) = expr
            && call.method == "children"
        {
            let previous = std::mem::replace(&mut self.builder_root, true);
            self.visit_expr_mut(&mut call.receiver);
            self.builder_root = previous;
            for arg in &mut call.args {
                if let Expr::Closure(closure) = arg
                    && closure.inputs.len() == 1
                    && let Some(Pat::Ident(binding)) = closure.inputs.first()
                {
                    let previous = std::mem::replace(&mut self.ui, binding.ident.clone());
                    self.visit_expr_mut(&mut closure.body);
                    self.ui = previous;
                } else {
                    self.visit_expr_mut(arg);
                }
            }
            return;
        }
        if let Expr::Macro(m) = expr
            && m.mac
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "text")
        {
            *expr = self.formatted_text(&m.mac);
            return;
        }
        visit_mut::visit_expr_mut(self, expr);
        if let Expr::MethodCall(call) = expr
            && self.is_ui(&call.receiver)
        {
            let name = call.method.to_string();
            if name == "add" && call.args.len() == 1 {
                let site = self.site();
                let runtime = &self.runtime;
                call.method = Ident::new("add_keyed", call.method.span());
                call.args
                    .insert(0, parse_quote!(#runtime::Key::Generated(#site)));
                return;
            }
            if matches!(
                name.as_str(),
                "slider"
                    | "shader"
                    | "modal"
                    | "popover"
                    | "tooltip"
                    | "menu"
                    | "menu_item"
                    | "select"
                    | "text_input"
                    | "virtual_list"
                    | "column"
                    | "row"
                    | "text"
                    | "text_fmt"
                    | "button"
                    | "button_base"
                    | "svg"
                    | "svg_source"
                    | "image"
            ) {
                let site = self.site();
                let runtime = &self.runtime;
                *expr = parse_quote!(#runtime::ImplicitKey::site_id(#call, #site));
            } else if matches!(name.as_str(), "switch" | "checkbox") && call.args.len() == 2 {
                let site = self.site();
                let runtime = &self.runtime;
                call.method = Ident::new(&format!("{name}_keyed"), call.method.span());
                call.args
                    .insert(0, parse_quote!(#runtime::Key::Generated(#site)));
            }
        } else if let Expr::MethodCall(call) = expr
            && call.args.iter().any(|arg| self.is_ui(arg))
        {
            let site = self.site();
            let ui = &self.ui;
            let original: Expr = parse_quote!(#call);
            *expr = scoped_call(&original, ui, site);
        } else if let Expr::Call(call) = expr
            && call.args.iter().any(|arg| self.is_ui(arg))
        {
            let constructor = if let Expr::Path(path) = &*call.func {
                path.path.segments.last().is_some_and(|segment| {
                    matches!(
                        segment.ident.to_string().as_str(),
                        "slider"
                            | "shader"
                            | "modal"
                            | "popover"
                            | "tooltip"
                            | "menu"
                            | "menu_item"
                            | "select"
                            | "text_input"
                            | "virtual_list"
                            | "column"
                            | "row"
                            | "text"
                            | "text_fmt"
                            | "button"
                            | "button_base"
                            | "svg"
                            | "image"
                            | "avatar"
                            | "ruby_text"
                            | "svg_source"
                            | "icon"
                            | "switch"
                            | "checkbox"
                    )
                })
            } else {
                false
            };
            let site = self.site();
            let runtime = &self.runtime;
            if constructor || self.builder_root {
                // A borrowing builder cannot escape a temporary scope closure.
                // Attach its call-site identity directly, just like Ui methods.
                *expr = parse_quote!(#runtime::ImplicitKey::site_id(#call, #site));
            } else {
                let ui = &self.ui;
                let original: Expr = parse_quote!(#call);
                *expr = scoped_call(&original, ui, site);
            }
        }
    }
    // Local function bodies are independent authoring boundaries.
    fn visit_item_mut(&mut self, _item: &mut syn::Item) {}
}

/// Format UI text using a reusable context buffer, with ordinary format_args! syntax.
#[proc_macro]
pub fn text(input: TokenStream) -> TokenStream {
    let tokens: Tokens = input.clone().into();
    if matches!(tokens.into_iter().next(),Some(proc_macro2::TokenTree::Punct(p)) if p.as_char()=='@')
    {
        return match syn::parse::<Expanded>(input) {
            Ok(expanded) => expanded.0.into_token_stream().into(),
            Err(error) => error.into_compile_error().into(),
        };
    }
    formatted_call(parse_macro_input!(input as FormattedText))
        .into_token_stream()
        .into()
}

// Preserve ordinary Rust for IDE recovery when an attribute sees an unfinished
// function. The diagnostic still prevents compiling the untransformed function.
fn parse_component_function(input: TokenStream) -> Result<ItemFn, TokenStream> {
    syn::parse(input.clone()).map_err(|error| {
        let diagnostic = error.into_compile_error();
        let original: Tokens = input.into();
        quote!(#diagnostic #original).into()
    })
}

/// Assign component identities to ordinary Rust declarations and closures.
#[proc_macro_attribute]
pub fn component(args: TokenStream, input: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "component takes no arguments",
        )
        .into_compile_error()
        .into();
    }
    let mut function = match parse_component_function(input) {
        Ok(function) => function,
        Err(tokens) => return tokens,
    };
    if function.sig.asyncness.is_some()
        || function.sig.constness.is_some()
        || matches!(function.sig.safety, syn::Safety::Unsafe(_))
    {
        return syn::Error::new_spanned(
            &function.sig,
            "component requires a synchronous safe function",
        )
        .into_compile_error()
        .into();
    }
    let ui=function.sig.inputs.iter().find_map(|arg|match arg {
        FnArg::Typed(arg) if matches!(&*arg.ty,Type::Reference(reference) if reference.mutability.is_some() && matches!(&*reference.elem,Type::Path(path) if path.path.segments.last().is_some_and(|s|s.ident=="Ui" || s.ident=="UiContext"))) => match &*arg.pat {Pat::Ident(p)=>Some(p.ident.clone()),_=>None},
        _=>None,
    });
    let Some(ui) = ui else {
        return syn::Error::new_spanned(
            &function.sig,
            "component requires a named &mut UiContext (or Ui) parameter",
        )
        .into_compile_error()
        .into();
    };
    let name = &function.sig.ident;
    let runtime = runtime();
    let namespace = quote!(concat!(module_path!(), "::", stringify!(#name)));
    let mut expand = Expand {
        ui: ui.clone(),
        namespace: namespace.clone(),
        runtime: runtime.clone(),
        next: 0,
        builder_root: false,
    };
    expand.visit_block_mut(&mut function.block);
    let body = &function.block;
    let returns_unit = match &function.sig.output {
        syn::ReturnType::Default => true,
        syn::ReturnType::Type(_, ty) => {
            matches!(&**ty, Type::Tuple(tuple) if tuple.elems.is_empty())
        }
    };
    if returns_unit {
        // Container declarations return InnerResponse even in statement position;
        // a unit component discards the container's response wrapper.
        *function.block = parse_quote!({
            const COMPONENT: u64 = #runtime::macro_support::site_id(#namespace, u32::MAX);
            #ui.scope(COMPONENT, |#ui| {
                let _ = &#ui;
                let _ = #body;
            })
        });
    } else {
        *function.block = parse_quote!({
            const COMPONENT: u64 = #runtime::macro_support::site_id(#namespace, u32::MAX);
            #ui.scope(COMPONENT, |#ui| {
                let _ = &#ui;
                #body
            })
        });
    }
    quote!(#function).into()
}

struct Expanded(Expr);
impl Parse for Expanded {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![@]>()?;
        let marker: Ident = input.parse()?;
        if marker != "__plico_expanded" {
            return Err(syn::Error::new_spanned(marker, "invalid text expansion"));
        }
        let expr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("unexpected text expansion tokens"));
        }
        Ok(Self(expr))
    }
}

fn scoped_call(call: &Expr, ui: &Ident, site: Tokens) -> Expr {
    struct Escapes(bool);
    impl VisitMut for Escapes {
        fn visit_expr_mut(&mut self, expr: &mut Expr) {
            match expr {
                Expr::Return(_)
                | Expr::Break(_)
                | Expr::Continue(_)
                | Expr::Yield(_)
                | Expr::Await(_) => self.0 = true,
                Expr::Closure(_) | Expr::Async(_) => {}
                _ => visit_mut::visit_expr_mut(self, expr),
            }
        }
    }
    let mut check = Escapes(false);
    check.visit_expr_mut(&mut call.clone());
    if check.0 {
        let error = syn::Error::new_spanned(
            call,
            "evaluate return/break/continue/await arguments before calling a UI helper",
        )
        .into_compile_error();
        parse_quote!({#error})
    } else {
        parse_quote!(#ui.scope(#site, |#ui| #call))
    }
}

/// Derive Kind classification from per-variant `#[kind(...)]` annotations.
///
/// One annotation site per variant generates `family`, `interactive` and
/// `leaf`, so adding a kind cannot silently miss a classification function.
/// See `kind.rs` for the annotation grammar.
#[proc_macro_derive(KindClass, attributes(kind))]
pub fn kind_class(input: TokenStream) -> TokenStream {
    kind::expand_derive(input)
}

/// Derive field-level invalidation channels for retained-style structs.
///
/// Annotate fields with `#[invalidates(CHANNEL, ...)]`; the derive generates
/// `changes_from(self, old) -> Changes` and `layout_eq(self, other) -> bool`
/// from the same marks. See `invalidates.rs` for the row grammar.
#[proc_macro_derive(InvalidateChannels, attributes(invalidates))]
pub fn invalidate_channels(input: TokenStream) -> TokenStream {
    invalidates::expand_derive(input)
}

mod invalidates;
mod kind;

/// Generate ALL, id, label, optional SVG icon and from_id for a fieldless route enum.
/// Variant metadata: #[route(id = "home", label = "Home", icon = icons::HOME)].
#[proc_macro_derive(Route, attributes(route))]
pub fn route(input: TokenStream) -> TokenStream {
    route::expand(parse_macro_input!(input as syn::DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
mod route;
