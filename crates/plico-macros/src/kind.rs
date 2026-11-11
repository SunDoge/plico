//! Kind classification derive: one annotation site per variant generates
//! every classification function, so adding a kind cannot silently miss one.
//!
//! ```ignore
//! #[derive(KindClass)]
//! pub enum Kind {
//!     #[kind]
//!     Root,
//!     #[kind(interactive, leaf)]
//!     Button,
//! }
//! ```
//!
//! `interactive` and `leaf` default to absent (false). Omitting the attribute
//! on a variant is a compile error, not a silent default.
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::{
    Ident, ItemEnum, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
    spanned::Spanned,
};

#[derive(Default)]
struct VariantAttr {
    interactive: bool,
    leaf: bool,
}

fn parse_kind_attr(attr: &syn::Attribute) -> syn::Result<VariantAttr> {
    if matches!(attr.meta, syn::Meta::Path(_)) {
        Ok(VariantAttr::default())
    } else {
        attr.parse_args::<VariantAttr>()
    }
}
impl Parse for VariantAttr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut interactive = false;
        let mut leaf = false;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "interactive" => interactive = true,
                "leaf" => leaf = true,
                other => {
                    return Err(input.error(format!(
                        "unknown kind property `{other}`; expected interactive or leaf"
                    )));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(VariantAttr { interactive, leaf })
    }
}

pub fn expand_derive(input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemEnum);
    let enum_name = &item.ident;

    let mut errors = Vec::new();
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
                format!("kind `{}` needs a #[kind(...)] annotation", variant.ident),
            ));
            continue;
        };
        match parse_kind_attr(attr) {
            Ok(_) => {}
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
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
                    .and_then(|attr| parse_kind_attr(attr).ok())
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
