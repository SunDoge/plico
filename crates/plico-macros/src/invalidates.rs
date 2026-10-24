//! Field-level invalidation channels derived from `#[invalidates(...)]` marks.
//!
//! Applied to a struct whose fields are `Copy + PartialEq`, the derive
//! generates `fn changes_from(self, old: Self) -> <struct's crate>::Changes`
//! and, when every field is annotated (or unannotated fields are explicitly
//! declared state-only), `fn layout_eq(self, other: Self) -> bool` covering
//! exactly the fields marked with `LAYOUT`.
//!
//! Field syntax:
//!
//! ```ignore
//! #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]          // plain field
//! #[invalidates(pad, LAYOUT, SEMANTICS)]               // pad => padding field
//! ```
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{format_ident, quote};
use syn::{
    Expr, Fields, Ident, ItemStruct, Token,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    spanned::Spanned,
};

/// One row of the parsed attribute: an optional source field id plus channels.
struct Row {
    field: Option<Ident>,
    channels: Vec<Ident>,
    getter: Option<Ident>,
}
impl Parse for Row {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut row = Row {
            field: None,
            channels: Vec::new(),
            getter: None,
        };
        // `source as getter` heads the row: the comparison calls that getter
        // on both sides.
        if input.peek(Ident) && input.peek2(Token![as]) {
            row.field = Some(input.parse()?);
            input.parse::<Token![as]>()?;
            row.getter = Some(input.parse()?);
            let _ = input.parse::<Option<Token![,]>>()?;
        }
        loop {
            if input.peek(Ident) {
                let channel: Ident = input.parse()?;
                if !channel
                    .to_string()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
                {
                    return Err(input.error(
                        "channel names are SCREAMING_CASE; field mapping uses `field as getter`",
                    ));
                }
                row.channels.push(channel);
                let _ = input.parse::<Option<Token![,]>>()?;
                continue;
            }
            break;
        }
        if row.channels.is_empty() {
            return Err(input.error("expected at least one invalidation channel"));
        }
        Ok(row)
    }
}
fn runtime_changes() -> Ident {
    format_ident!("Changes")
}

pub fn expand_derive(input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemStruct);
    let Fields::Named(fields) = &item.fields else {
        return syn::Error::new(item.span(), "InvalidateChannels requires named fields")
            .into_compile_error()
            .into();
    };
    let struct_name = &item.ident;
    let runtime = runtime_path();
    let changes = runtime_changes();
    let mut rows: Vec<(Expr, Vec<Ident>, bool, Ident)> = Vec::new(); // (expr self, channels, in_layout)
    let mut errors = Vec::new();
    let mut property_constants = Vec::new();
    for (index, field) in fields.named.iter().enumerate() {
        // Several marks on one field are allowed; plain-field channels union,
        // and each getter mark contributes its own comparison row.
        let attrs: Vec<&syn::Attribute> = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("invalidates"))
            .collect();
        if attrs.is_empty() {
            continue; // state-only field: no invalidation
        }
        let field_name = field.ident.as_ref().unwrap();
        if index >= 64 {
            return syn::Error::new(field.span(), "property masks support up to 64 fields")
                .into_compile_error()
                .into();
        }
        let property = format_ident!("{}", field_name.to_string().to_uppercase());
        let bit = 1u64 << index;
        property_constants.push(quote!(const #property = #bit;));
        let mut parsed_rows = Vec::new();
        let mut row_errors = Vec::new();
        for attr in attrs {
            match attr.parse_args::<Row>() {
                Ok(row) => parsed_rows.push(row),
                Err(error) => row_errors.push(error),
            }
        }
        if let Some(first) = row_errors.first() {
            errors.push(first.clone());
            continue;
        }
        let mut plain_channels: Vec<Ident> = Vec::new();
        let mut getter_rows = Vec::new();
        for row in parsed_rows {
            match row.getter {
                Some(getter) => getter_rows.push((getter, row.channels)),
                None => {
                    let _ = row.field;
                    plain_channels.extend(row.channels);
                }
            }
        }
        if !plain_channels.is_empty() {
            let access: Expr = parse_quote!(self.#field_name);
            let in_layout = plain_channels.iter().any(|c| c == "LAYOUT");
            rows.push((access, plain_channels, in_layout, property.clone()));
        }
        for (getter, channels) in getter_rows {
            let access: Expr = parse_quote!(self.#getter());
            let in_layout = channels.iter().any(|c| c == "LAYOUT");
            rows.push((access, channels, in_layout, property.clone()));
        }
    }
    if let Some(first) = errors.first() {
        return first.to_compile_error().into();
    }

    let properties_name = format_ident!("{}Properties", struct_name);
    let diff_name = format_ident!("{}Diff", struct_name);
    let changes_from = {
        let arms = rows.iter().map(|(access, channels, _, property)| {
            let flags = channels.iter().map(|c| quote!(#runtime::#changes::#c));
            let old_access = access_to_old(access, "old");
            quote! {
                if #access != #old_access {
                    diff.changes |= #( #flags )|*;
                    diff.properties |= #properties_name::#property;
                }
            }
        });
        quote! {
            /// Compute work channels and exact style properties in one pass.
            pub fn diff(self, old: Self) -> #diff_name {
                let mut diff = #diff_name::default();
                #( #arms )*
                diff
            }
            pub fn changes_from(self, old: Self) -> #runtime::#changes {
                self.diff(old).changes
            }
        }
    };
    let layout_eq = {
        let checks =
            rows.iter()
                .filter(|(_, _, in_layout, _)| *in_layout)
                .map(|(access, _, _, _)| {
                    let old_access = access_to_old(access, "other");
                    quote!( #access == #old_access )
                });
        quote! {
            /// Whether layout inputs match; generated from the fields whose
            /// `#[invalidates]` marks include `LAYOUT`.
            pub fn layout_eq(self, other: Self) -> bool {
                true #( && #checks )*
            }
        }
    };
    let expanded = quote! {
        #runtime::macro_support::bitflags! {
            /// Exact style inputs changed; generated from the same field marks as work channels.
            #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
            pub struct #properties_name: u64 { #( #property_constants )* }
        }
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        pub struct #diff_name {
            pub changes: #runtime::#changes,
            pub properties: #properties_name,
        }
        impl #struct_name {
            #changes_from
            #layout_eq
        }
    };
    expanded.into()
}
fn access_to_old(access: &Expr, other_name: &str) -> Expr {
    // Rebind `self` receivers to the `old` parameter for the comparison side.
    match access {
        Expr::Field(expr) => {
            let mut cloned = expr.clone();
            cloned.base = format_ident_receiver(other_name);
            Expr::Field(cloned)
        }
        Expr::MethodCall(expr) => {
            let mut cloned = expr.clone();
            cloned.receiver = format_ident_receiver(other_name);
            Expr::MethodCall(cloned)
        }
        other => other.clone(),
    }
}
fn format_ident_receiver(name: &str) -> Box<Expr> {
    let ident = Ident::new(name, proc_macro2::Span::call_site());
    Box::new(parse_quote!(#ident))
}
fn runtime_path() -> Tokens {
    match proc_macro_crate::crate_name("plico-core") {
        Ok(proc_macro_crate::FoundCrate::Name(name)) => {
            let name = Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#name)
        }
        _ => quote!(::plico_core),
    }
}
