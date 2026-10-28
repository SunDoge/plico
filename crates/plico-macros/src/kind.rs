//! Kind classification derive: one annotation site per variant generates
//! every classification function, so adding a kind cannot silently miss one.
//!
//! ```ignore
//! #[derive(KindClass)]
//! pub enum Kind {
//!     #[kind(family = Container)]
//!     Root,
//!     #[kind(family = Control, interactive, leaf)]
//!     Button,
//! }
//! ```
//!
//! `family` is required; `interactive` and `leaf` default to absent (false).
//! Omitting the attribute on a variant is a compile error, not a silent
//! default — the whole point of the derive.
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::{
    Ident, ItemEnum, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
    spanned::Spanned,
};

struct VariantAttr {
    family: Ident,
    interactive: bool,
    leaf: bool,
}
impl Parse for VariantAttr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut family = None;
        let mut interactive = false;
        let mut leaf = false;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "family" => {
                    input.parse::<Token![=]>()?;
                    let value: Ident = input.parse()?;
                    if family.replace(value).is_some() {
                        return Err(input.error("duplicate `family`"));
                    }
                }
                "interactive" => interactive = true,
                "leaf" => leaf = true,
                other => {
                    return Err(input.error(format!(
                        "unknown kind property `{other}`; expected family, interactive or leaf"
                    )));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        let Some(family) = family else {
            return Err(input.error("every kind needs `family = ...`"));
        };
        Ok(VariantAttr {
            family,
            interactive,
            leaf,
        })
    }
}

pub fn expand_derive(input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemEnum);
    let runtime = runtime_path();
    let enum_name = &item.ident;

    let mut errors = Vec::new();
    let mut arms_family = Vec::new();
    let mut arms_interactive = Vec::new();
    let mut arms_leaf = Vec::new();

    for variant in &item.variants {
        let attrs: Vec<&syn::Attribute> = variant
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("kind"))
            .collect();
        if attrs.len() > 1 {
            errors.push(syn::Error::new(
                variant.span(),
                "duplicate #[kind] attribute",
            ));
            continue;
        }
        let Some(attr) = attrs.first() else {
            errors.push(syn::Error::new(
                variant.span(),
                format!(
                    "kind `{}` needs a #[kind(...)] annotation with its family",
                    variant.ident
                ),
            ));
            continue;
        };
        let parsed = match attr.parse_args::<VariantAttr>() {
            Ok(parsed) => parsed,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
        let ident = &variant.ident;
        let family = &parsed.family;
        arms_family.push(quote! {
            #enum_name::#ident => #runtime::KindFamily::#family,
        });
        if parsed.interactive {
            arms_interactive.push(quote! { #enum_name::#ident | });
        }
        if parsed.leaf {
            arms_leaf.push(quote! { #enum_name::#ident | });
        }
    }
    if let Some(first) = errors.first() {
        return first.to_compile_error().into();
    }

    // Collect variant idents per flag once; matches! arms are built from
    // these lists so trailing separators stay correct when a flag has no
    // members (an empty matches! would not compile — a false wildcard arm
    // covers that case).
    let flag_idents = |flag: fn(&VariantAttr) -> bool| -> Vec<Tokens> {
        item.variants
            .iter()
            .filter_map(|v| {
                v.attrs
                    .iter()
                    .find(|attr| attr.path().is_ident("kind"))
                    .and_then(|attr| attr.parse_args::<VariantAttr>().ok())
                    .filter(flag)
                    .map(|_| {
                        let ident = &v.ident;
                        quote! { #enum_name::#ident }
                    })
            })
            .collect()
    };
    let interactive_idents = flag_idents(|a| a.interactive);
    let leaf_idents = flag_idents(|a| a.leaf);
    let interactive_tail = if interactive_idents.is_empty() {
        quote! { _ if false }
    } else {
        quote! { #(#interactive_idents)|* }
    };
    let leaf_tail = if leaf_idents.is_empty() {
        quote! { _ if false }
    } else {
        quote! { #(#leaf_idents)|* }
    };

    let expanded = quote! {
        impl #enum_name {
            /// The family a backend without a native widget for this kind
            /// projects it onto; generated from per-variant #[kind]
            /// annotations.
            pub const fn family(self) -> #runtime::KindFamily {
                match self {
                    #(#arms_family)*
                }
            }
            /// Whether this kind participates in interaction routing;
            /// generated from #[kind(interactive)].
            pub const fn interactive(self) -> bool {
                matches!(self, #interactive_tail)
            }
            /// Whether this kind is a layout leaf; generated from
            /// #[kind(leaf)].
            pub(crate) fn leaf(self) -> bool {
                matches!(self, #leaf_tail)
            }
        }
    };
    expanded.into()
}

fn runtime_path() -> Tokens {
    crate::runtime()
}
