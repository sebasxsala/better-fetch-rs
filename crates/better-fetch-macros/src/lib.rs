//! Proc-macro helpers for [`better-fetch`](https://docs.rs/better-fetch).
//!
//! Enable the `macros` feature on `better-fetch` to use these derives.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse_macro_input, spanned::Spanned, Attribute, Data, DeriveInput, Fields, LitStr, Meta,
};

fn endpoint_path_attr(attrs: &[Attribute]) -> syn::Result<LitStr> {
    for attr in attrs {
        if !attr.path().is_ident("endpoint") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            return Err(syn::Error::new(attr.span(), "`#[endpoint]` must be a list"));
        };
        let mut found = None;
        list.parse_nested_meta(|meta| {
            if meta.path.is_ident("path") {
                let value = meta.value()?;
                found = Some(value.parse::<LitStr>()?);
            }
            Ok(())
        })?;
        if let Some(path) = found {
            return Ok(path);
        }
        return Err(syn::Error::new(
            attr.span(),
            "`#[endpoint]` requires `path = \"...\"`",
        ));
    }
    Err(syn::Error::new(
        Span::call_site(),
        "`#[derive(EndpointParams)]` requires `#[endpoint(path = \"/route/:param\")]`",
    ))
}

fn param_key(field: &syn::Field) -> syn::Result<String> {
    for attr in &field.attrs {
        if !attr.path().is_ident("param") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let mut rename = None;
        list.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                rename = Some(value.parse::<LitStr>()?.value());
            }
            Ok(())
        })?;
        if let Some(name) = rename {
            return Ok(name);
        }
    }
    let ident = field
        .ident
        .as_ref()
        .ok_or_else(|| syn::Error::new(field.span(), "tuple struct fields are not supported"))?;
    Ok(ident.to_string())
}

fn path_param_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| segment.strip_prefix(':').map(str::to_string))
        .collect()
}

/// Derives [`EndpointParams`](https://docs.rs/better-fetch/latest/better_fetch/trait.EndpointParams.html)
/// for a struct with one field per `:param` segment in `#[endpoint(path = "...")]`.
///
/// Optional `#[param(rename = "segmentName")]` overrides the path segment for a field.
#[proc_macro_derive(EndpointParams, attributes(endpoint, param))]
pub fn derive_endpoint_params(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_endpoint_params_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_endpoint_params_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let path = endpoint_path_attr(&input.attrs)?;
    let path_value = path.value();

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`EndpointParams` can only be derived for structs",
        ));
    };

    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            data.fields.span(),
            "`EndpointParams` requires a struct with named fields",
        ));
    };

    let mut field_keys = Vec::new();
    let mut apply_pairs = Vec::new();

    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let key = param_key(field)?;
        field_keys.push(key.clone());
        apply_pairs.push(quote! {
            builder = builder.param(#key, self.#ident);
        });
    }

    let expected = path_param_names(&path_value);
    if expected.len() != field_keys.len() {
        return Err(syn::Error::new(
            path.span(),
            format!(
                "path `{path_value}` has {} `:param` segment(s) but the struct has {} field(s)",
                expected.len(),
                field_keys.len()
            ),
        ));
    }

    for segment in expected {
        if !field_keys.iter().any(|key| key == &segment) {
            return Err(syn::Error::new(
                path.span(),
                format!("missing struct field for path parameter `:{segment}`"),
            ));
        }
    }

    Ok(quote! {
        impl ::better_fetch::EndpointParams for #name {
            type BuilderState = ::better_fetch::NeedsParams;

            fn apply_params(
                self,
                mut builder: ::better_fetch::RequestBuilder<'_>,
            ) -> ::better_fetch::RequestBuilder<'_> {
                #(#apply_pairs)*
                builder
            }
        }
    })
}

/// Derives [`EndpointQuery`](https://docs.rs/better-fetch/latest/better_fetch/trait.EndpointQuery.html)
/// for a serde-serializable query struct.
///
/// Requires `Serialize` on the type (typically via `#[derive(Serialize)]`).
#[proc_macro_derive(EndpointQuery, attributes(query))]
pub fn derive_endpoint_query(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_endpoint_query_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_endpoint_query_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`EndpointQuery` can only be derived for structs",
        ));
    };

    if !matches!(data.fields, Fields::Named(_)) {
        return Err(syn::Error::new(
            data.fields.span(),
            "`EndpointQuery` requires a struct with named fields",
        ));
    }

    Ok(quote! {
        impl ::better_fetch::EndpointQuery for #name {
            fn apply_query(
                self,
                builder: ::better_fetch::RequestBuilder<'_>,
            ) -> ::better_fetch::RequestBuilder<'_> {
                ::better_fetch::endpoint::apply_serialized_query(self, builder)
            }
        }
    })
}
