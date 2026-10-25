//! Metadata for finite, fieldless navigation enums. No navigation or rendering code.
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::HashSet;
use syn::{Data, DeriveInput, Fields, LitStr, Path};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Route requires an enum",
        ));
    };
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "Route does not support generic enums",
        ));
    }
    if input.attrs.iter().any(|a| a.path().is_ident("route")) {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "place #[route(...)] on variants",
        ));
    }
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Route requires at least one variant",
        ));
    }
    let mut ids = HashSet::new();
    let mut variants = Vec::new();
    let mut id_arms = Vec::new();
    let mut label_arms = Vec::new();
    let mut icon_arms = Vec::new();
    let mut parse_arms = Vec::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "Route supports only fieldless variants; parameterized routes are not enumerable navigation entries",
            ));
        }
        let name = &variant.ident;
        let mut id: Option<LitStr> = None;
        let mut label: Option<LitStr> = None;
        let mut icon: Option<Path> = None;
        for attr in variant.attrs.iter().filter(|a| a.path().is_ident("route")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("id") {
                    if id.is_some() {
                        return Err(meta.error("duplicate route id"));
                    }
                    id = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("label") {
                    if label.is_some() {
                        return Err(meta.error("duplicate route label"));
                    }
                    label = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("icon") {
                    if icon.is_some() {
                        return Err(meta.error("duplicate route icon"));
                    }
                    icon = Some(meta.value()?.parse()?);
                } else {
                    return Err(meta.error("expected id, label or icon"));
                }
                Ok(())
            })?;
        }
        let fallback = name.to_string();
        let fallback = fallback.trim_start_matches("r#");
        let id = id.unwrap_or_else(|| LitStr::new(fallback, name.span()));
        let label = label.unwrap_or_else(|| LitStr::new(fallback, name.span()));
        if id.value().trim().is_empty() {
            return Err(syn::Error::new_spanned(id, "route id must not be empty"));
        }
        if !ids.insert(id.value()) {
            return Err(syn::Error::new_spanned(id, "route ids must be unique"));
        }
        let icon = icon.map_or_else(
            || quote!(::core::option::Option::None),
            |path| quote!(::core::option::Option::Some(&#path)),
        );
        variants.push(quote!(Self::#name));
        id_arms.push(quote!(Self::#name => #id));
        label_arms.push(quote!(Self::#name => #label));
        icon_arms.push(quote!(Self::#name => #icon));
        parse_arms.push(quote!(#id => ::core::option::Option::Some(Self::#name)));
    }
    let name = &input.ident;
    let runtime = crate::runtime();
    Ok(quote! {
        impl #name {
            /// Navigation entries in declaration order. No allocation.
            pub const ALL: &'static [Self] = &[#(#variants),*];
            /// Stable logical ID. Defaults to the variant name; override before renaming.
            pub const fn id(&self) -> &'static str { match self { #(#id_arms),* } }
            /// Display label; defaults to the variant name.
            pub const fn label(&self) -> &'static str { match self { #(#label_arms),* } }
            /// Optional application-owned SVG asset.
            pub const fn icon(&self) -> ::core::option::Option<&'static #runtime::SvgAsset> {
                match self { #(#icon_arms),* }
            }
            /// Look up an exact logical ID. Unknown IDs return None.
            pub fn from_id(id: &str) -> ::core::option::Option<Self> {
                match id { #(#parse_arms),*, _ => ::core::option::Option::None }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;
    #[test]
    fn rejects_ambiguous_or_unsupported_routes() {
        let cases: Vec<(DeriveInput, &str)> = vec![
            (
                parse_quote!(
                    struct Page;
                ),
                "requires an enum",
            ),
            (
                parse_quote!(
                    enum Page {}
                ),
                "at least one variant",
            ),
            (
                parse_quote!(
                    enum Page {
                        Project(u32),
                    }
                ),
                "fieldless",
            ),
            (
                parse_quote!(
                    enum Page {
                        Project { id: u32 },
                    }
                ),
                "fieldless",
            ),
            (
                parse_quote!(
                    enum Page<T> {
                        Home,
                    }
                ),
                "generic",
            ),
            (
                parse_quote!(
                    enum Page {
                        #[route(id = "x")]
                        A,
                        #[route(id = "x")]
                        B,
                    }
                ),
                "unique",
            ),
            (
                parse_quote!(
                    enum Page {
                        A,
                        #[route(id = "A")]
                        B,
                    }
                ),
                "unique",
            ),
            (
                parse_quote!(
                    enum Page {
                        #[route(id = " ")]
                        A,
                    }
                ),
                "empty",
            ),
            (
                parse_quote!(
                    enum Page {
                        #[route(label = "a", label = "b")]
                        A,
                    }
                ),
                "duplicate",
            ),
            (
                parse_quote!(
                    enum Page {
                        #[route(id = "a")]
                        #[route(id = "b")]
                        A,
                    }
                ),
                "duplicate",
            ),
            (
                parse_quote!(
                    enum Page {
                        #[route(path = "/")]
                        A,
                    }
                ),
                "expected id",
            ),
            (
                parse_quote!(
                    #[route(label = "x")]
                    enum Page {
                        A,
                    }
                ),
                "on variants",
            ),
        ];
        for (input, expected) in cases {
            assert!(
                expand(input).unwrap_err().to_string().contains(expected),
                "{expected}"
            );
        }
    }
}
