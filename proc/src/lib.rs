#[proc_macro_derive(Asset, attributes(asset, serde))]
pub fn asset(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match parse(item).and_then(asset_impl) {
        Ok(tokens) => tokens,
        Err(error) => error.into_compile_error(),
    }
    .into()
}

#[proc_macro_derive(AssetField, attributes(asset, serde))]
pub fn asset_field(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match parse(item).and_then(asset_field_impl) {
        Ok(tokens) => tokens,
        Err(error) => error.into_compile_error(),
    }
    .into()
}

struct Parsed {
    complex: bool,
    derive_input: syn::DeriveInput,
    info: syn::Ident,
    futures: syn::Ident,
    decoded: syn::Ident,
    decode_error: syn::Ident,
    decode_field_errors: proc_macro2::TokenStream,
    build_error: syn::Ident,
    build_field_errors: proc_macro2::TokenStream,
    builder_bounds: proc_macro2::TokenStream,
    info_fields: proc_macro2::TokenStream,
    info_to_futures_fields: proc_macro2::TokenStream,
    futures_fields: proc_macro2::TokenStream,
    futures_to_decoded_fields: proc_macro2::TokenStream,
    decoded_fields: proc_macro2::TokenStream,
    decoded_to_asset_fields: proc_macro2::TokenStream,
    serde_attributes: Vec<syn::Attribute>,
    name: Option<syn::LitStr>,
}

fn parse(item: proc_macro::TokenStream) -> syn::Result<Parsed> {
    use syn::spanned::Spanned;

    let derive_input = syn::parse::<syn::DeriveInput>(item)?;

    let asset_attributes = derive_input
        .attrs
        .iter()
        .enumerate()
        .filter_map(|(index, attr)| {
            if attr.path.is_ident("asset") {
                Some(index)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let mut name_arg = None;

    for idx in &asset_attributes {
        let attr = &derive_input.attrs[*idx];

        attr.parse_args_with(|stream: syn::parse::ParseStream| {
            match stream.parse::<syn::Ident>()? {
                i if i == "name" => {
                    let _eq = stream.parse::<syn::Token![=]>()?;

                    let name = stream.parse::<syn::LitStr>()?;
                    name_arg = Some(name);

                    if !stream.is_empty() {
                        return Err(syn::Error::new(stream.span(), "Expected end of arguments"));
                    }

                    Ok(())
                }
                i => Err(syn::Error::new_spanned(
                    i,
                    "Unexpected ident. Expected: 'name'",
                )),
            }
        })?;
    }

    let serde_attributes = derive_input
        .attrs
        .iter()
        .filter(|attr| attr.path.is_ident("serde"))
        .cloned()
        .collect();

    let mut decode_field_errors = proc_macro2::TokenStream::new();
    let mut build_field_errors = proc_macro2::TokenStream::new();
    let mut builder_bounds = proc_macro2::TokenStream::new();

    let info = quote::format_ident!("{}Info", derive_input.ident);
    let mut info_fields = proc_macro2::TokenStream::new();
    let mut info_to_futures_fields = proc_macro2::TokenStream::new();

    let futures = quote::format_ident!("{}Futures", derive_input.ident);
    let mut futures_fields = proc_macro2::TokenStream::new();
    let mut futures_to_decoded_fields = proc_macro2::TokenStream::new();

    let decoded = quote::format_ident!("{}Decoded", derive_input.ident);
    let mut decoded_fields = proc_macro2::TokenStream::new();
    let mut decoded_to_asset_fields = proc_macro2::TokenStream::new();

    let decode_error = quote::format_ident!("{}DecodeError", derive_input.ident);
    let build_error = quote::format_ident!("{}BuildError", derive_input.ident);

    let mut complex: bool = false;

    let data_struct = match &derive_input.data {
        syn::Data::Struct(data) => data,
        syn::Data::Enum(data) => {
            return Err(syn::Error::new_spanned(
                data.enum_token,
                "Only structs are currently supported by derive(Asset) macro",
            ))
        }
        syn::Data::Union(data) => {
            return Err(syn::Error::new_spanned(
                data.union_token,
                "Only structs are currently supported by derive(Asset) macro",
            ))
        }
    };

    for (index, field) in data_struct.fields.iter().enumerate() {
        let asset_attributes = field
            .attrs
            .iter()
            .enumerate()
            .filter_map(|(index, attr)| {
                if attr.path.is_ident("asset") {
                    Some(index)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let serde_attributes = field
            .attrs
            .iter()
            .filter(|attr| attr.path.is_ident("serde"));

        let ty = &field.ty;

        match asset_attributes.len() {
            0 => match &field.ident {
                Some(ident) => {
                    info_fields.extend(quote::quote!(
                        #(#serde_attributes)*
                        pub #ident: #ty,
                    ));
                    futures_fields.extend(quote::quote!(pub #ident: #ty,));
                    decoded_fields.extend(quote::quote!(pub #ident: #ty,));
                    info_to_futures_fields.extend(quote::quote!(#ident: info.#ident,));
                    futures_to_decoded_fields.extend(quote::quote!(#ident: futures.#ident,));
                    decoded_to_asset_fields.extend(quote::quote!(#ident: decoded.#ident,));
                }
                None => {
                    info_fields.extend(quote::quote!(
                        #(#serde_attributes)*
                        pub #ty,
                    ));
                    futures_fields.extend(quote::quote!(pub #ty,));
                    decoded_fields.extend(quote::quote!(pub #ty,));
                    info_to_futures_fields.extend(quote::quote!(info.#index,));
                    futures_to_decoded_fields.extend(quote::quote!(futures.#index,));
                    decoded_to_asset_fields.extend(quote::quote!(decoded.#index,));
                }
            },
            1 => {
                complex = true;

                let mut is_external = false;
                let mut is_container = false;
                let mut as_type_arg = None;

                for idx in &asset_attributes {
                    let attribute = &field.attrs[*idx];

                    attribute.parse_args_with(|stream: syn::parse::ParseStream| {
                        match stream.parse::<syn::Ident>()? {
                            i if i == "external" => {
                                if is_container {
                                    return Err(syn::Error::new_spanned(i, "Attributes 'container' and 'external' are mutually exclusive"));
                                }
                                if is_external {
                                    return Err(syn::Error::new_spanned(i, "Attributes 'external' is already specified"));
                                }
                                is_external = true;

                                if !stream.is_empty() {
                                    let args;
                                    syn::parenthesized!(args in stream);
                                    let _as = args.parse::<syn::Token![as]>()?;
                                    let as_type = args.parse::<syn::Type>()?;
                                    as_type_arg = Some(as_type);

                                    if !stream.is_empty() {
                                        return Err(syn::Error::new(stream.span(), "Expected end of arguments"));
                                    }
                                }

                                Ok(())
                            },
                            i if i == "container" => {
                                if is_external {
                                    return Err(syn::Error::new_spanned(i, "Attributes 'external' and 'container' are mutually exclusive"));
                                }
                                if is_container {
                                    return Err(syn::Error::new_spanned(i, "Attributes 'container' is already specified"));
                                }
                                is_container = true;

                                if !stream.is_empty() {
                                    let args;
                                    syn::parenthesized!(args in stream);
                                    let _as = args.parse::<syn::Token![as]>()?;
                                    let as_type = args.parse::<syn::Type>()?;
                                    as_type_arg = Some(as_type);

                                    if !stream.is_empty() {
                                        return Err(syn::Error::new(stream.span(), "Expected end of arguments"));
                                    }
                                }

                                Ok(())
                            }
                            i => {
                                Err(syn::Error::new_spanned(i, "Unexpected ident. Expected: 'external' or 'container'"))
                            }
                        }
                    })?;
                }

                let as_type = as_type_arg.as_ref().unwrap_or(ty);

                let kind = match (is_container, is_external) {
                    (false, true) => quote::quote!(::goods::External),
                    (true, false) => quote::quote!(::goods::Container),
                    _ => unreachable!(),
                };

                match &field.ident {
                    Some(ident) => {
                        let error_variant = quote::format_ident!("{}Error", snake_to_pascal(ident));
                        let decode_error_text = syn::LitStr::new(
                            &format!("Failed to decode asset field '{}'", ident),
                            ident.span(),
                        );
                        let build_error_text = syn::LitStr::new(
                            &format!("Failed to build asset field '{}'", ident),
                            ident.span(),
                        );

                        decode_field_errors.extend(quote::quote!(
                            #[error(#decode_error_text)]
                            #error_variant { source: <#as_type as ::goods::AssetField<#kind>>::DecodeError },
                        ));
                        build_field_errors.extend(quote::quote!(
                            #[error(#build_error_text)]
                            #error_variant { source: <#as_type as ::goods::AssetField<#kind>>::BuildError },
                        ));

                        builder_bounds.extend(
                            quote::quote!(#as_type: ::goods::AssetFieldBuild<#kind, BuilderGenericParameter>,),
                        );
                        info_fields.extend(
                            quote::quote!(pub #ident: <#as_type as ::goods::AssetField<#kind>>::Info,),
                        );
                        futures_fields.extend(
                            quote::quote!(pub #ident: <#as_type as ::goods::AssetField<#kind>>::Fut,),
                        );
                        decoded_fields
                            .extend(quote::quote!(pub #ident: <#as_type as ::goods::AssetField<#kind>>::Decoded,));
                        info_to_futures_fields
                            .extend(quote::quote!(#ident: <#as_type as ::goods::AssetField<#kind>>::decode(info.#ident, loader),));
                        futures_to_decoded_fields
                            .extend(quote::quote!(#ident: futures.#ident.await.map_err(|err| #decode_error::#error_variant { source: err })?,));
                        decoded_to_asset_fields
                            .extend(quote::quote!(#ident: <#ty as ::std::convert::From<#as_type>>::from(<#as_type as ::goods::AssetFieldBuild<#kind, BuilderGenericParameter>>::build(decoded.#ident, builder).map_err(|err| #build_error::#error_variant { source: err })?),));
                    }
                    None => {
                        let error_variant =
                            syn::Ident::new(&format!("Field{}Error", index), field.span());
                        let decode_error_text = syn::LitStr::new(
                            &format!("Failed to decode asset field '{}'", index),
                            field.span(),
                        );
                        let build_error_text = syn::LitStr::new(
                            &format!("Failed to load asset field '{}'", index),
                            field.span(),
                        );

                        decode_field_errors.extend(quote::quote!(
                            #[error(#decode_error_text)]
                            #error_variant { source: <#as_type as ::goods::AssetField<#kind>>::DecodeError },
                        ));
                        build_field_errors.extend(quote::quote!(
                            #[error(#build_error_text)]
                            #error_variant { source: <#as_type as ::goods::AssetField<#kind>>::BuildError },
                        ));

                        builder_bounds.extend(
                            quote::quote!(#as_type: ::goods::AssetFieldBuild<#kind, BuilderGenericParameter>,),
                        );
                        info_fields.extend(
                            quote::quote!(pub <#as_type as ::goods::AssetField<#kind>>::Info,),
                        );
                        futures_fields.extend(
                            quote::quote!(pub <#as_type as ::goods::AssetField<#kind>>::Fut,),
                        );
                        decoded_fields.extend(
                            quote::quote!(pub <#as_type as ::goods::AssetField<#kind>>::Decoded,),
                        );
                        info_to_futures_fields
                            .extend(quote::quote!(<#as_type as ::goods::AssetField<#kind>>::decode(info.#index, loader),));
                        futures_to_decoded_fields.extend(quote::quote!(futures.#index.await.map_err(|err| #decode_error::#error_variant { source: err })?,));
                        decoded_to_asset_fields
                            .extend(quote::quote!(<#ty as ::std::convert::From<#as_type>>::from(<#as_type as ::goods::AssetFieldBuild<#kind, BuilderGenericParameter>>::build(decoded.#index, builder).map_err(|err| #build_error::#error_variant { source: err })?),));
                    }
                }
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    &field.attrs[asset_attributes[1]],
                    "Only one of two attributes 'external' or 'container' can be specified",
                ));
            }
        }
    }

    Ok(Parsed {
        complex,
        derive_input,
        info,
        futures,
        decoded,
        decode_error,
        decode_field_errors,
        build_error,
        build_field_errors,
        builder_bounds,
        info_fields,
        info_to_futures_fields,
        futures_fields,
        futures_to_decoded_fields,
        decoded_fields,
        decoded_to_asset_fields,
        serde_attributes,
        name: name_arg,
    })
}

fn asset_impl(parsed: Parsed) -> syn::Result<proc_macro2::TokenStream> {
    let Parsed {
        complex,
        derive_input,
        info,
        futures,
        decoded,
        decode_error,
        build_error,
        decode_field_errors,
        build_field_errors,
        builder_bounds,
        info_fields,
        info_to_futures_fields,
        futures_fields,
        futures_to_decoded_fields,
        decoded_fields,
        decoded_to_asset_fields,
        serde_attributes,
        name,
    } = parsed;

    let name = match name {
        None => {
            return Err(syn::Error::new_spanned(
                derive_input,
                "`derive(Asset)` requires `asset(name = \"<name>\")` attribute",
            ));
        }
        Some(name) => name,
    };

    let data_struct = match &derive_input.data {
        syn::Data::Struct(data) => data,
        _ => unreachable!(),
    };

    let ty = &derive_input.ident;

    let tokens = match data_struct.fields {
        syn::Fields::Unit => quote::quote! {
            #[derive(::goods::serde::Serialize, ::goods::serde::Deserialize)]
            #(#serde_attributes)*
            pub struct #info;

            impl ::goods::TrivialAsset for #ty {
                type Error = ::std::convert::Infallible;

                fn name() -> &'static str {
                    #name
                }

                fn decode(bytes: ::std::boxed::Box<[u8]>) -> Result<Self, ::std::convert::Infallible> {
                    ::std::result::Result::Ok(#ty)
                }
            }

            impl ::goods::AssetField<::goods::Container> for #ty {
                type BuildError = ::std::convert::Infallible;
                type DecodeError = ::std::convert::Infallible;
                type Info = #info;
                type Decoded = Self;
                type Fut = ::std::future::Ready<Result<Self, ::std::convert::Infallible>>;

                fn decode(info: #info, _: &::goods::Loader) -> Self::Fut {
                    use ::std::{future::ready, result::Result::Ok};

                    ready(Ok(#ty))
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetFieldBuild<::goods::Container, BuilderGenericParameter> for #ty {
                fn build(decoded: Self, builder: &mut BuilderGenericParameter) -> Result<Self, ::std::convert::Infallible> {
                    ::std::result::Result::Ok(decoded)
                }
            }
        },
        syn::Fields::Unnamed(_) => todo!("Not yet implemented"),
        syn::Fields::Named(_) if complex => quote::quote! {
            #[derive(::goods::serde::Serialize, ::goods::serde::Deserialize)]
            #(#serde_attributes)*
            pub struct #info { #info_fields }

            pub struct #futures { #futures_fields }

            pub struct #decoded { #decoded_fields }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #decode_error {
                #[error("Failed to deserialize asset info. {source:#}")]
                Info { #[source] source: ::goods::DecodeError },

                #decode_field_errors
            }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #build_error {
                #build_field_errors
            }

            impl ::goods::Asset for #ty {
                type BuildError = #build_error;
                type DecodeError = #decode_error;
                type Decoded = #decoded;
                type Fut = ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = ::std::result::Result<#decoded, #decode_error>> + Send>>;

                fn name() -> &'static str {
                    #name
                }

                fn decode(bytes: ::std::boxed::Box<[u8]>, loader: &::goods::Loader) -> Self::Fut {
                    use {::std::{boxed::Box, result::Result::{self, Ok, Err}}, ::goods::serde_json::error::Category};

                    // Zero-length is definitely bincode.
                    let result: Result<#info, #decode_error> = if bytes.is_empty()  {
                        match ::goods::bincode::deserialize(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => Err(#decode_error:: Info { source: ::goods::DecodeError::Bincode(err) }),
                        }
                    } else {
                        match ::goods::serde_json::from_slice(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => match err.classify() {
                                Category::Syntax => {
                                    // That's not json. Bincode then.
                                    match ::goods::bincode::deserialize(&*bytes) {
                                        Ok(value) => Ok(value),
                                        Err(err) => Err(#decode_error:: Info { source: ::goods::DecodeError::Bincode(err) }),
                                    }
                                }
                                _ => Err(#decode_error::Info { source: ::goods::DecodeError::Json(err) }),
                            }
                        }
                    };

                    match result {
                        Ok(info) => {
                            let futures = #futures {
                                #info_to_futures_fields
                            };
                            Box::pin(async move {Ok(#decoded {
                                #futures_to_decoded_fields
                            })})
                        },
                        Err(err) => Box::pin(async move { Err(err) }),
                    }
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetBuild<BuilderGenericParameter> for #ty
            where
                #builder_bounds
            {
                fn build(decoded: #decoded, builder: &mut BuilderGenericParameter) -> Result<Self, #build_error> {
                    ::std::result::Result::Ok(#ty {
                        #decoded_to_asset_fields
                    })
                }
            }

            impl ::goods::AssetField<::goods::Container> for #ty {
                type BuildError = #build_error;
                type DecodeError = #decode_error;
                type Info = #info;
                type Decoded = #decoded;
                type Fut = ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = Result<#decoded, #decode_error>> + Send>>;

                fn decode(info: #info, loader: &::goods::Loader) -> Self::Fut {
                    use ::std::{boxed::Box, result::Result::Ok};

                    struct #futures { #futures_fields }

                    let futures = #futures {
                        #info_to_futures_fields
                    };

                    Box::pin(async move {Ok(#decoded {
                        #futures_to_decoded_fields
                    })})
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetFieldBuild<::goods::Container, BuilderGenericParameter> for #ty
            where
                #builder_bounds
            {
                fn build(decoded: #decoded, builder: &mut BuilderGenericParameter) -> Result<Self, #build_error> {
                    ::std::result::Result::Ok(#ty {
                        #decoded_to_asset_fields
                    })
                }
            }
        },
        syn::Fields::Named(_) => quote::quote! {
            #[derive(::goods::serde::Serialize, ::goods::serde::Deserialize)]
            #(#serde_attributes)*
            pub struct #info { #info_fields }

            impl ::goods::TrivialAsset for #ty {
                type Error = ::goods::DecodeError;

                fn name() -> &'static str {
                    #name
                }

                fn decode(bytes: ::std::boxed::Box<[u8]>) -> Result<Self, ::goods::DecodeError> {
                    use {::std::result::Result::{Ok, Err}, ::goods::serde_json::error::Category};

                    /// Zero-length is definitely bincode.
                    let decoded: #info = if bytes.is_empty()  {
                        match ::goods::bincode::deserialize(&*bytes) {
                            Ok(value) => value,
                            Err(err) => return Err(::goods::DecodeError::Bincode(err)),
                        }
                    } else {
                        match ::goods::serde_json::from_slice(&*bytes) {
                            Ok(value) => value,
                            Err(err) => match err.classify() {
                                Category::Syntax => {
                                    // That's not json. Bincode then.
                                    match ::goods::bincode::deserialize(&*bytes) {
                                        Ok(value) => value,
                                        Err(err) => return Err(::goods::DecodeError::Bincode(err)),
                                    }
                                }
                                _ => return Err(::goods::DecodeError::Json(err)),
                            }
                        }
                    };

                    Ok(#ty {
                        #decoded_to_asset_fields
                    })
                }
            }

            impl ::goods::AssetField<::goods::Container> for #ty {
                type BuildError = ::std::convert::Infallible;
                type DecodeError = ::std::convert::Infallible;
                type Info = #info;
                type Decoded = Self;
                type Fut = ::std::future::Ready<Result<Self, ::std::convert::Infallible>>;

                fn decode(info: #info, _: &::goods::Loader) -> Self::Fut {
                    use ::std::{future::ready, result::Result::Ok};

                    let decoded = info;

                    ready(Ok(#ty {
                        #decoded_to_asset_fields
                    }))
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetFieldBuild<::goods::Container, BuilderGenericParameter> for #ty {
                fn build(decoded: Self, builder: &mut BuilderGenericParameter) -> Result<Self, ::std::convert::Infallible> {
                    ::std::result::Result::Ok(decoded)
                }
            }
        },
    };

    Ok(tokens)
}

fn asset_field_impl(parsed: Parsed) -> syn::Result<proc_macro2::TokenStream> {
    let Parsed {
        complex,
        derive_input,
        info,
        futures,
        decoded,
        decode_error,
        build_error,
        decode_field_errors,
        build_field_errors,
        builder_bounds,
        info_fields,
        info_to_futures_fields,
        futures_fields,
        futures_to_decoded_fields,
        decoded_fields,
        decoded_to_asset_fields,
        serde_attributes,
        name,
    } = parsed;

    if let Some(name) = name {
        return Err(syn::Error::new_spanned(
            name,
            "`derive(AssetField)` does not accept `asset(name = \"<name>\")` attribute",
        ));
    };

    let ty = &derive_input.ident;

    let data_struct = match &derive_input.data {
        syn::Data::Struct(data) => data,
        _ => unreachable!(),
    };

    let tokens = match data_struct.fields {
        syn::Fields::Unit => quote::quote! {
            #[derive(::goods::serde::Serialize, ::goods::serde::Deserialize)]
            #(#serde_attributes)*
            pub struct #info;

            impl ::goods::AssetField<::goods::Container> for #ty {
                type BuildError = ::std::convert::Infallible;
                type DecodeError = ::std::convert::Infallible;
                type Info = #info;
                type Decoded = Self;
                type Fut = ::std::future::Ready<Result<Self, ::std::convert::Infallible>>;

                fn decode(info: #info, _: &::goods::Loader) -> Self::Fut {
                    use ::std::{future::ready, result::Result::Ok};

                    ready(Ok(#ty))
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetFieldBuild<::goods::Container, BuilderGenericParameter> for #ty {
                fn build(decoded: Self, builder: &mut BuilderGenericParameter) -> Result<Self, ::std::convert::Infallible> {
                    ::std::result::Result::Ok(decoded)
                }
            }
        },

        syn::Fields::Unnamed(_) => todo!("Not yet implemented"),
        syn::Fields::Named(_) if complex => quote::quote! {
            #[derive(::goods::serde::Serialize, ::goods::serde::Deserialize)]
            #(#serde_attributes)*
            pub struct #info { #info_fields }

            pub struct #decoded { #decoded_fields }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #decode_error {
                #decode_field_errors
            }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #build_error {
                #build_field_errors
            }

            impl ::goods::AssetField<::goods::Container> for #ty {
                type BuildError = #build_error;
                type DecodeError = #decode_error;
                type Info = #info;
                type Decoded = #decoded;
                type Fut = ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = Result<#decoded, #decode_error>> + Send>>;

                fn decode(info: #info, loader: &::goods::Loader) -> Self::Fut {
                    use ::std::{boxed::Box, result::Result::Ok};

                    struct #futures { #futures_fields }

                    let futures = #futures {
                        #info_to_futures_fields
                    };

                    Box::pin(async move {Ok(#decoded {
                        #futures_to_decoded_fields
                    })})
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetFieldBuild<::goods::Container, BuilderGenericParameter> for #ty
            where
                #builder_bounds
            {
                fn build(decoded: #decoded, builder: &mut BuilderGenericParameter) -> Result<Self, #build_error> {
                    ::std::result::Result::Ok(#ty {
                        #decoded_to_asset_fields
                    })
                }
            }
        },
        syn::Fields::Named(_) => quote::quote! {
            #[derive(::goods::serde::Serialize, ::goods::serde::Deserialize)]
            #(#serde_attributes)*
            pub struct #info { #info_fields }

            impl ::goods::AssetField<::goods::Container> for #ty {
                type BuildError = ::std::convert::Infallible;
                type DecodeError = ::std::convert::Infallible;
                type Info = #info;
                type Decoded = Self;
                type Fut = ::std::future::Ready<Result<Self, ::std::convert::Infallible>>;

                fn decode(info: #info, _: &::goods::Loader) -> Self::Fut {
                    use ::std::{future::ready, result::Result::Ok};

                    let decoded = info;

                    ready(Ok(#ty {
                        #decoded_to_asset_fields
                    }))
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetFieldBuild<::goods::Container, BuilderGenericParameter> for #ty {
                fn build(decoded: Self, builder: &mut BuilderGenericParameter) -> Result<Self, ::std::convert::Infallible> {
                    ::std::result::Result::Ok(decoded)
                }
            }
        },
    };

    Ok(tokens)
}

fn snake_to_pascal(input: &syn::Ident) -> syn::Ident {
    let mut result = String::new();
    let mut upper = true;
    for char in input.to_string().chars() {
        if char.is_ascii_alphabetic() {
            if upper {
                upper = false;
                result.extend(char.to_uppercase());
            } else {
                result.push(char);
            }
        } else if char.is_ascii_digit() {
            upper = true;
            result.push(char);
        } else if char == '_' {
            upper = true;
        } else {
            return input.clone();
        }
    }
    syn::Ident::new(&result, input.span())
}
