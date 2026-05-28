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

    let mut seen_keys = std::collections::HashSet::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let key = param_key(field)?;
        if !seen_keys.insert(key.clone()) {
            return Err(syn::Error::new(
                field.span(),
                format!("duplicate path parameter `{key}`"),
            ));
        }
        field_keys.push(key.clone());
        apply_pairs.push(quote! {
            builder = builder.param(#key, self.#ident);
        });
    }

    let expected = path_param_names(&path_value);
    let mut seen_segments = std::collections::HashSet::new();
    for segment in &expected {
        if !seen_segments.insert(segment.clone()) {
            return Err(syn::Error::new(
                path.span(),
                format!("duplicate `:param` segment `:{segment}` in path"),
            ));
        }
    }
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
            ) -> ::better_fetch::Result<::better_fetch::RequestBuilder<'_>> {
                ::better_fetch::endpoint::apply_serialized_query(self, builder)
            }
        }
    })
}

fn endpoint_meta(
    attrs: &[Attribute],
) -> syn::Result<(proc_macro2::TokenStream, LitStr, bool, bool)> {
    for attr in attrs {
        if !attr.path().is_ident("endpoint") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            return Err(syn::Error::new(attr.span(), "`#[endpoint]` must be a list"));
        };
        let mut method = None;
        let mut path = None;
        let mut register = false;
        list.parse_nested_meta(|meta| {
            if meta.path.is_ident("method") {
                let value = meta.value()?;
                method = Some(value.parse::<syn::Path>()?);
            } else if meta.path.is_ident("path") {
                let value = meta.value()?;
                path = Some(value.parse::<LitStr>()?);
            } else if meta.path.is_ident("register") {
                register = true;
            }
            Ok(())
        })?;
        let method_path = method.ok_or_else(|| {
            syn::Error::new(attr.span(), "`#[endpoint]` requires `method = GET` (etc.)")
        })?;
        let path = path.ok_or_else(|| {
            syn::Error::new(attr.span(), "`#[endpoint]` requires `path = \"...\"`")
        })?;
        let is_post = method_path.get_ident().is_some_and(|id| id == "POST")
            || method_path
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "POST");
        let method = if let Some(ident) = method_path.get_ident() {
            quote!(::http::Method::#ident)
        } else {
            quote!(#method_path)
        };
        return Ok((method, path, is_post, register));
    }
    Err(syn::Error::new(
        Span::call_site(),
        "`#[derive(Endpoint)]` requires `#[endpoint(method = GET, path = \"...\")]`",
    ))
}

fn is_unit_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Tuple(t) if t.elems.is_empty())
}

fn endpoint_field_type(field: &syn::Field, attr: &str) -> Option<syn::Type> {
    field
        .attrs
        .iter()
        .any(|a| a.path().is_ident(attr))
        .then(|| field.ty.clone())
}

/// Derives [`Endpoint`](https://docs.rs/better-fetch/latest/better_fetch/trait.Endpoint.html).
///
/// ```ignore
/// #[derive(Endpoint)]
/// #[endpoint(method = GET, path = "/items/:id")]
/// struct GetItem {
///     #[response]
///     Item,
///     #[params]
///     ItemParams,
/// }
/// ```
#[proc_macro_derive(
    Endpoint,
    attributes(endpoint, response, params, query, body, headers, param, query_field)
)]
pub fn derive_endpoint(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_endpoint_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_endpoint_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (method, path, is_post, register) = endpoint_meta(&input.attrs)?;
    let path_value = path.value();

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`Endpoint` can only be derived for structs",
        ));
    };

    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            data.fields.span(),
            "`Endpoint` requires a struct with named fields for `#[response]` etc.",
        ));
    };

    let mut response = quote!(());
    let mut params = quote!(());
    let mut query = quote!(());
    let mut body = quote!(());
    let mut headers = quote!(());
    let mut body_ty: Option<syn::Type> = None;
    let mut inline_param_fields: Vec<&syn::Field> = Vec::new();
    let mut inline_query_fields: Vec<&syn::Field> = Vec::new();
    let mut explicit_params = false;
    let mut explicit_query = false;

    for field in &fields.named {
        if field.attrs.iter().any(|a| a.path().is_ident("param")) {
            inline_param_fields.push(field);
            continue;
        }
        if field.attrs.iter().any(|a| a.path().is_ident("query_field")) {
            inline_query_fields.push(field);
            continue;
        }
        if let Some(ty) = endpoint_field_type(field, "response") {
            response = quote!(#ty);
        } else if let Some(ty) = endpoint_field_type(field, "params") {
            explicit_params = true;
            params = quote!(#ty);
        } else if let Some(ty) = endpoint_field_type(field, "query") {
            explicit_query = true;
            query = quote!(#ty);
        } else if let Some(ty) = endpoint_field_type(field, "body") {
            body_ty = Some(ty.clone());
            body = quote!(#ty);
        } else if let Some(ty) = endpoint_field_type(field, "headers") {
            headers = quote!(#ty);
        }
    }

    if explicit_params && !inline_param_fields.is_empty() {
        return Err(syn::Error::new(
            input.span(),
            "use either `#[params] Type` or `#[param]` fields on the endpoint struct, not both",
        ));
    }

    if explicit_query && !inline_query_fields.is_empty() {
        return Err(syn::Error::new(
            input.span(),
            "use either `#[query] Type` or `#[query_field]` fields on the endpoint struct, not both",
        ));
    }

    let params_ty_ident = syn::Ident::new(&format!("{name}Params"), name.span());
    let query_ty_ident = syn::Ident::new(&format!("{name}Query"), name.span());
    let inline_params_impl = if !inline_param_fields.is_empty() {
        let mut field_defs = Vec::new();
        let mut apply_pairs = Vec::new();
        let mut field_keys = Vec::new();
        let mut seen_keys = std::collections::HashSet::new();

        for field in &inline_param_fields {
            let ident = field.ident.as_ref().expect("named field");
            let key = param_key(field)?;
            if !seen_keys.insert(key.clone()) {
                return Err(syn::Error::new(
                    field.span(),
                    format!("duplicate path parameter `{key}`"),
                ));
            }
            field_keys.push(key.clone());
            let ty = &field.ty;
            field_defs.push(quote! { pub #ident: #ty });
            apply_pairs.push(quote! {
                builder = builder.param(#key, self.#ident);
            });
        }

        let expected = path_param_names(&path_value);
        if expected.len() != field_keys.len() {
            return Err(syn::Error::new(
                path.span(),
                format!(
                    "path `{path_value}` has {} `:param` segment(s) but the endpoint has {} `#[param]` field(s)",
                    expected.len(),
                    field_keys.len()
                ),
            ));
        }
        for segment in expected {
            if !field_keys.iter().any(|key| key == &segment) {
                return Err(syn::Error::new(
                    path.span(),
                    format!("missing `#[param]` field for path parameter `:{segment}`"),
                ));
            }
        }

        params = quote!(#params_ty_ident);
        quote! {
            #[derive(Debug, Clone, Default)]
            pub struct #params_ty_ident {
                #(#field_defs),*
            }

            impl ::better_fetch::EndpointParams for #params_ty_ident {
                type BuilderState = ::better_fetch::NeedsParams;

                fn apply_params(
                    self,
                    mut builder: ::better_fetch::RequestBuilder<'_>,
                ) -> ::better_fetch::RequestBuilder<'_> {
                    #(#apply_pairs)*
                    builder
                }
            }
        }
    } else {
        quote! {}
    };

    let inline_query_impl = if !inline_query_fields.is_empty() {
        let mut field_defs = Vec::new();
        for field in &inline_query_fields {
            let ident = field.ident.as_ref().expect("named field");
            let ty = &field.ty;
            field_defs.push(quote! { pub #ident: #ty });
        }
        query = quote!(#query_ty_ident);
        quote! {
            #[derive(Debug, Clone, Default, ::serde::Serialize)]
            pub struct #query_ty_ident {
                #(#field_defs),*
            }

            impl ::better_fetch::EndpointQuery for #query_ty_ident {
                fn apply_query(
                    self,
                    builder: ::better_fetch::RequestBuilder<'_>,
                ) -> ::better_fetch::Result<::better_fetch::RequestBuilder<'_>> {
                    ::better_fetch::endpoint::apply_serialized_query(self, builder)
                }
            }
        }
    } else {
        quote! {}
    };

    let explicit_query_impl = if explicit_query && inline_query_fields.is_empty() {
        quote! {
            impl ::better_fetch::EndpointQuery for #query {
                fn apply_query(
                    self,
                    builder: ::better_fetch::RequestBuilder<'_>,
                ) -> ::better_fetch::Result<::better_fetch::RequestBuilder<'_>> {
                    ::better_fetch::endpoint::apply_serialized_query(self, builder)
                }
            }
        }
    } else {
        quote! {}
    };

    let body_required = is_post && body_ty.as_ref().is_some_and(|ty| !is_unit_type(ty));

    let body_required_impl = if let Some(body_type) = body_ty.filter(|_| body_required) {
        quote! {
            impl ::better_fetch::EndpointBody for #body_type {
                type ParamsNext = ::better_fetch::NeedsBody;
                type CallInitial = ::better_fetch::NeedsBody;

                fn apply_body(
                    self,
                    builder: ::better_fetch::RequestBuilder<'_>,
                ) -> ::better_fetch::Result<::better_fetch::RequestBuilder<'_>> {
                    builder.json(&self)
                }
            }

            impl ::better_fetch::DefaultParamsInitial<#name> for () {
                fn initial(
                    client: &::better_fetch::Client,
                ) -> ::better_fetch::EndpointRequestBuilder<'_, #name, ::better_fetch::NeedsBody> {
                    ::better_fetch::EndpointRequestBuilder::new_needs_body(
                        client.request(#method, #path_value),
                    )
                }
            }
        }
    } else {
        quote! {}
    };

    let register_impl = if register {
        quote! {
            impl #name {
                /// Registers this route in a [`SchemaRegistry`](::better_fetch::SchemaRegistry).
                #[cfg(feature = "schema")]
                pub fn register(registry: &mut ::better_fetch::SchemaRegistry) {
                    registry.register_typed::<#name, #body, #response>();
                }
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #inline_params_impl
        #inline_query_impl
        #explicit_query_impl
        impl ::better_fetch::Endpoint for #name {
            const METHOD: ::http::Method = #method;
            const PATH: &'static str = #path_value;
            type Response = #response;
            type Params = #params;
            type Query = #query;
            type Body = #body;
            type Headers = #headers;
        }
        #body_required_impl
        #register_impl
    })
}
