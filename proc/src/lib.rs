///
/// Creates structures to act as two stages first of asset and implement asset using those.
/// First stages must be deserializable with serde. For this, all non-asset fields of the target struct
/// must implement `DeserializeOwned`. In turn asset fields will be replaced with uuid for first stage struct.
/// Second stages will have `AssetResult`s fields in place of the assets.
///
/// This works recursively.
///
/// # Example
///
/// ```
/// /// Asset field type. Additional structures are generated, but no `Asset` implementation.
/// /// Fields of types with `#[asset_field]` attribute are not replaced by uuids as external assets.
/// #[asset_field]
/// struct Foo {
///   bar: Bar,
/// }
///
/// /// Simple deserializable type. Included as-is into generated types for `#[asset]`.
/// #[serde::Deserialize]
/// struct Bar {}
///
/// /// Another asset type.
/// #[asset]
/// struct Baz {}
///
/// /// Asset structure. Implements Asset trait using
/// /// two generated structures are intermediate phases.
/// #[asset]
/// struct AssetStruct {
///     foo: Foo,
///     bar: Bar,
///     #[external]
///     baz: Baz,
/// }
/// ```
///
#[proc_macro_derive(Asset, attributes(external, container))]
pub fn asset(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match parse(item) {
        Ok(parsed) => asset_impl(parsed),
        Err(error) => error.into_compile_error(),
    }
    .into()
}

#[proc_macro_derive(AssetContainer, attributes(external, container))]
pub fn asset_container(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match parse(item).and_then(|parsed| asset_container_impl(parsed)) {
        Ok(tokens) => tokens,
        Err(error) => error.into_compile_error(),
    }
    .into()
}

struct Parsed {
    complex: bool,
    derive_input: syn::DeriveInput,
    error: syn::Ident,
    info: syn::Ident,
    futures: syn::Ident,
    decoded: syn::Ident,
    field_errors: proc_macro2::TokenStream,
    builder_bounds: proc_macro2::TokenStream,
    info_fields: proc_macro2::TokenStream,
    info_to_futures_fields: proc_macro2::TokenStream,
    futures_fields: proc_macro2::TokenStream,
    futures_to_decoded_fields: proc_macro2::TokenStream,
    decoded_fields: proc_macro2::TokenStream,
    decoded_to_asset_fields: proc_macro2::TokenStream,
}

fn parse(item: proc_macro::TokenStream) -> syn::Result<Parsed> {
    use syn::spanned::Spanned;

    let derive_input = syn::parse::<syn::DeriveInput>(item)?;

    let mut field_errors = proc_macro2::TokenStream::new();
    let mut builder_bounds = proc_macro2::TokenStream::new();

    let info = quote::format_ident!("{}Info", derive_input.ident);
    let mut info_fields = proc_macro2::TokenStream::new();
    let mut info_to_futures_fields = proc_macro2::TokenStream::new();

    let futures = quote::format_ident!("{}futures", derive_input.ident);
    let mut futures_fields = proc_macro2::TokenStream::new();
    let mut futures_to_decoded_fields = proc_macro2::TokenStream::new();

    let decoded = quote::format_ident!("{}Decoded", derive_input.ident);
    let mut decoded_fields = proc_macro2::TokenStream::new();
    let mut decoded_to_asset_fields = proc_macro2::TokenStream::new();

    let error = quote::format_ident!("{}AssetError", derive_input.ident);

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
        let external_attribute = field.attrs.iter().position(|attr| {
            attr.path
                .get_ident()
                .map_or(false, |ident| ident == "external")
        });
        let container_attribute = field.attrs.iter().position(|attr| {
            attr.path
                .get_ident()
                .map_or(false, |ident| ident == "container")
        });

        let ty = &field.ty;
        match (&external_attribute, &container_attribute) {
            (&Some(external), &Some(container)) => {
                return Err(syn::Error::new_spanned(
                    &field.attrs[external.max(container)],
                    "Only one of two attributes 'external' or 'container' can be specified",
                ));
            }
            (&Some(_), None) => {
                complex = true;
                match &field.ident {
                    Some(ident) => {
                        let error_variant = quote::format_ident!("{}Error", snake_to_pascal(ident));
                        let error_text = syn::LitStr::new(
                            &format!("Failed to load sub-asset field '{}'", ident),
                            ident.span(),
                        );
                        field_errors.extend(quote::quote!(
                            #[error(#error_text)]
                            #error_variant { source: ::goods::Error },
                        ));
                        builder_bounds
                            .extend(quote::quote!(#ty: ::goods::AssetBuild<GoodsAssetBuilder>,));
                        info_fields.extend(quote::quote!(#ident: ::goods::Uuid,));
                        futures_fields.extend(quote::quote!(#ident: ::goods::AssetHandle<#ty>,));
                        decoded_fields.extend(quote::quote!(#ident: ::goods::AssetResult<#ty>,));
                        info_to_futures_fields
                            .extend(quote::quote!(#ident: loader.load(&info.#ident),));
                        futures_to_decoded_fields
                            .extend(quote::quote!(#ident: futures.#ident.await,));
                        decoded_to_asset_fields
                            .extend(quote::quote!(#ident: decoded.#ident.get(builder).map_err(|err| #error::#error_variant { source: err })?.clone(),));
                    }
                    None => {
                        let error_variant =
                            syn::Ident::new(&format!("Field{}Error", index), field.span());
                        let error_text = syn::LitStr::new(
                            &format!("Failed to load sub-asset field '{}'", index),
                            field.span(),
                        );
                        field_errors.extend(quote::quote!(
                            #[error(#error_text)]
                            #error_variant { source: ::goods::Error }
                        ));
                        builder_bounds
                            .extend(quote::quote!(#ty: ::goods::AssetBuild<GoodsAssetBuilder>,));
                        info_fields.extend(quote::quote!(::goods::Uuid,));
                        futures_fields.extend(quote::quote!(::goods::AssetHandle<#ty>,));
                        decoded_fields.extend(quote::quote!(::goods::AssetResult<#ty>,));
                        info_to_futures_fields.extend(quote::quote!(loader.load(&info.#index),));
                        futures_to_decoded_fields.extend(quote::quote!(futures.#index.await,));
                        decoded_to_asset_fields
                            .extend(quote::quote!(decoded.#index.get(builder).map_err(|err| #error::#error_variant { source: err })?.clone(),));
                    }
                }
            }
            (None, &Some(_)) => {
                complex = true;
                match &field.ident {
                    Some(ident) => {
                        let error_variant = quote::format_ident!("{}Error", snake_to_pascal(ident));
                        let error_text = syn::LitStr::new(
                            &format!("Failed to load sub-asset container field '{}'", ident),
                            ident.span(),
                        );
                        field_errors.extend(quote::quote!(
                            #[error(#error_text)]
                            #error_variant { source: <#ty as ::goods::AssetContainer>::Error },
                        ));
                        builder_bounds.extend(
                            quote::quote!(#ty: ::goods::AssetContainerBuild<GoodsAssetBuilder>,),
                        );
                        info_fields
                            .extend(quote::quote!(#ident: <#ty as ::goods::AssetContainer>::Info,));
                        futures_fields
                            .extend(quote::quote!(#ident: <#ty as ::goods::AssetContainer>::Fut,));
                        decoded_fields.extend(
                            quote::quote!(#ident: <#ty as ::goods::AssetContainer>::Decoded,),
                        );
                        info_to_futures_fields.extend(
                                quote::quote!(#ident: <#ty as ::goods::AssetContainer>::decode(info.#ident, loader),),
                            );
                        futures_to_decoded_fields
                            .extend(quote::quote!(#ident: futures.#ident.await.map_err(|err| #error::#error_variant { source: err })?,));
                        decoded_to_asset_fields
                            .extend(quote::quote!(#ident: <#ty as ::goods::AssetContainerBuild<_>>::build(decoded.#ident, builder).map_err(|err| #error::#error_variant { source: err })?,));
                    }
                    None => {
                        let error_variant =
                            syn::Ident::new(&format!("Field{}Error", index), field.span());
                        let error_text = syn::LitStr::new(
                            &format!("Failed to load sub-asset container field '{}'", index),
                            field.span(),
                        );
                        field_errors.extend(quote::quote!(
                            #[error(#error_text)]
                            #error_variant { source: <#ty as ::goods::AssetContainer>::Error }
                        ));
                        builder_bounds.extend(
                            quote::quote!(#ty: ::goods::AssetContainerBuild<GoodsAssetBuilder>,),
                        );
                        info_fields.extend(quote::quote!(<#ty as ::goods::AssetContainer>::Info,));
                        futures_fields.extend(quote::quote!(<#ty as ::goods::Asset>::Fut,));
                        decoded_fields
                            .extend(quote::quote!(<#ty as ::goods::AssetContainer>::Decoded,));
                        info_to_futures_fields.extend(
                            quote::quote!(<#ty as ::goods::AssetContainer>::decode(info.#index, loader),),
                        );
                        futures_to_decoded_fields.extend(quote::quote!(futures.#index.await.map_err(|err| #error::#error_variant { source: err })?,));
                        decoded_to_asset_fields.extend(
                            quote::quote!(<#ty as ::goods::AssetContainerBuild<_>>::build(decoded.#index, builder).map_err(|err| #error::#error_variant { source: err })?,),
                        );
                    }
                }
            }
            (None, None) => match &field.ident {
                Some(ident) => {
                    info_fields.extend(quote::quote!(#ident: #ty,));
                    futures_fields.extend(quote::quote!(#ident: #ty,));
                    decoded_fields.extend(quote::quote!(#ident: #ty,));
                    info_to_futures_fields.extend(quote::quote!(#ident: info.#ident,));
                    futures_to_decoded_fields.extend(quote::quote!(#ident: futures.#ident,));
                    decoded_to_asset_fields.extend(quote::quote!(#ident: decoded.#ident,));
                }
                None => {
                    info_fields.extend(quote::quote!(#ty,));
                    futures_fields.extend(quote::quote!(#ty,));
                    decoded_fields.extend(quote::quote!(#ty,));
                    info_to_futures_fields.extend(quote::quote!(info.#index,));
                    futures_to_decoded_fields.extend(quote::quote!(futures.#index,));
                    decoded_to_asset_fields.extend(quote::quote!(decoded.#index,));
                }
            },
        }
    }

    Ok(Parsed {
        complex,
        derive_input,
        error,
        info,
        futures,
        decoded,
        field_errors,
        builder_bounds,
        info_fields,
        info_to_futures_fields,
        futures_fields,
        futures_to_decoded_fields,
        decoded_fields,
        decoded_to_asset_fields,
    })
}

fn asset_impl(parsed: Parsed) -> proc_macro2::TokenStream {
    let Parsed {
        complex,
        derive_input,
        error,
        info,
        futures,
        decoded,
        field_errors,
        builder_bounds,
        info_fields,
        info_to_futures_fields,
        futures_fields,
        futures_to_decoded_fields,
        decoded_fields,
        decoded_to_asset_fields,
    } = parsed;

    let data_struct = match &derive_input.data {
        syn::Data::Struct(data) => data,
        _ => unreachable!(),
    };

    let ty = &derive_input.ident;

    let tokens = match data_struct.fields {
        syn::Fields::Unit => quote::quote! {
            #[derive(::goods::serde::Deserialize)]
            pub struct #decoded;

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #error {
                #[error("Failed to deserialize asset from json")]
                Json(#[source] ::goods::serde_json::Error),

                #[error("Failed to deserialize asset from bincode")]
                Bincode(#[source] ::goods::bincode::Error),
            }

            impl ::goods::Asset for #ty {
                type Error = #error;
                type Decoded = #decoded;
                type Fut = ::std::future::Ready<::std::result::Result<#decoded, #error>>;

                fn decode(bytes: ::std::boxed::Box<[u8]>, _loader: &::goods::Loader) -> Self::Fut {
                    use {::std::result::Result::{Ok, Err}, ::goods::serde_json::error::Category};

                    /// Zero-length is definitely bincode. For unit structs we may skip deserialization.
                    if bytes.is_empty() {
                        return ready(Ok(#decoded))
                    }

                    let result = match ::goods::serde_json::from_slice(&*bytes) {
                        Ok(value) => Ok(value),
                        Err(err) => match err.classify() {
                            Category::Syntax => {
                                // That's not json. Bincode then.
                                match ::goods::bincode::deserialize(&*bytes) {
                                    Ok(value) => Ok(value),
                                    Err(err) => Err(#error::Bincode(err)),
                                }
                            }
                            _ => {
                                Err(#error::Json(err))
                            }
                        }
                    };

                    ::std::future::ready(result)
                }
            }

            impl<GoodsAssetBuilder> ::goods::AssetBuild<GoodsAssetBuilder> for #ty {
                fn build(#decoded: #decoded, _builder: &mut GoodsAssetBuilder) -> ::std::result::Result<Self, #error> {
                    Ok(#ty)
                }
            }
        },
        syn::Fields::Unnamed(_) => todo!("Not yet implemented"),
        syn::Fields::Named(_) if complex => quote::quote! {
            #[derive(::goods::serde::Deserialize)]
            struct #info { #info_fields }

            struct #futures { #futures_fields }

            pub struct #decoded { #decoded_fields }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #error {
                #[error("Failed to deserialize asset info from json")]
                Json(#[source] ::goods::serde_json::Error),

                #[error("Failed to deserialize asset info from bincode")]
                Bincode(#[source] ::goods::bincode::Error),

                #field_errors
            }

            impl ::goods::Asset for #ty {
                type Error = #error;
                type Decoded = #decoded;
                type Fut = ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = Result<#decoded, #error>> + Send>>;

                fn decode(bytes: ::std::boxed::Box<[u8]>, loader: &::goods::Loader) -> Self::Fut {
                    use {::std::{boxed::Box, result::Result::{self, Ok, Err}}, ::goods::serde_json::error::Category};

                    // Zero-length is definitely bincode.
                    let result: Result<#info, #error> = if bytes.is_empty()  {
                        match ::goods::bincode::deserialize(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => Err(#error::Bincode(err)),
                        }
                    } else {
                        match ::goods::serde_json::from_slice(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => match err.classify() {
                                Category::Syntax => {
                                    // That's not json. Bincode then.
                                    match ::goods::bincode::deserialize(&*bytes) {
                                        Ok(value) => Ok(value),
                                        Err(err) => Err(#error::Bincode(err)),
                                    }
                                }
                                _ => {
                                    Err(#error::Json(err))
                                }
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

            impl<GoodsAssetBuilder> ::goods::AssetBuild<GoodsAssetBuilder> for #ty
            where
                #builder_bounds
            {
                fn build(mut decoded: #decoded, builder: &mut GoodsAssetBuilder) -> Result<Self, #error> {
                    ::std::result::Result::Ok(#ty {
                        #decoded_to_asset_fields
                    })
                }
            }
        },
        syn::Fields::Named(_) => quote::quote! {
            #[derive(::goods::serde::Deserialize)]
            pub struct #decoded { #decoded_fields }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #error {
                #[error("Failed to deserialize asset info from json")]
                Json(#[source] ::goods::serde_json::Error),

                #[error("Failed to deserialize asset info from bincode")]
                Bincode(#[source] ::goods::bincode::Error),
            }

            impl ::goods::Asset for #ty {
                type Error = #error;
                type Decoded = #decoded;
                type Fut = ::std::future::Ready<Result<#decoded, #error>>;

                fn decode(bytes: ::std::boxed::Box<[u8]>, _loader: &::goods::Loader) -> Self::Fut {
                    use {::std::result::Result::{Ok, Err}, ::goods::serde_json::error::Category};

                    /// Zero-length is definitely bincode.
                    let result = if bytes.is_empty()  {
                        match ::goods::bincode::deserialize(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => Err(#error::Bincode(err)),
                        }
                    } else {
                        match ::goods::serde_json::from_slice(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => match err.classify() {
                                Category::Syntax => {
                                    // That's not json. Bincode then.
                                    match ::goods::bincode::deserialize(&*bytes) {
                                        Ok(value) => Ok(value),
                                        Err(err) => Err(#error::Bincode(err)),
                                    }
                                }
                                _ => {
                                    Err(#error::Json(err))
                                }
                            }
                        }
                    };

                    ::std::future::ready(result)
                }
            }

            impl<GoodsAssetBuilder> ::goods::AssetBuild<GoodsAssetBuilder> for #ty {
                fn build(decoded: #decoded, _builder: &mut GoodsAssetBuilder) -> Result<Self, #error> {
                    Ok(#ty {
                        #decoded_to_asset_fields
                    })
                }
            }
        },
    };

    tokens
}

fn asset_container_impl(parsed: Parsed) -> syn::Result<proc_macro2::TokenStream> {
    let Parsed {
        complex,
        derive_input,
        error,
        info,
        futures,
        decoded,
        field_errors,
        builder_bounds,
        info_fields,
        info_to_futures_fields,
        futures_fields,
        futures_to_decoded_fields,
        decoded_fields,
        decoded_to_asset_fields,
    } = parsed;

    if !complex {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "Asset container contains no assets",
        ));
    }

    let ty = &derive_input.ident;

    let data_struct = match &derive_input.data {
        syn::Data::Struct(data) => data,
        _ => unreachable!(),
    };

    let tokens = match data_struct.fields {
        syn::Fields::Unit => unreachable!(),

        syn::Fields::Unnamed(_) => todo!("Not yet implemented"),
        syn::Fields::Named(_) if complex => quote::quote! {
            #[derive(::goods::serde::Deserialize)]
            pub struct #info { #info_fields }

            struct #futures { #futures_fields }

            pub struct #decoded { #decoded_fields }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #error {
                #field_errors
            }

            impl ::goods::AssetContainer for #ty {
                type Error = #error;
                type Info = #info;
                type Decoded = #decoded;
                type Fut = ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = Result<#decoded, #error>> + Send>>;

                fn decode(info: #info, loader: &::goods::Loader) -> Self::Fut {
                    use ::std::{boxed::Box, result::Result::Ok};

                    let futures = #futures {
                        #info_to_futures_fields
                    };
                    Box::pin(async move {Ok(#decoded {
                        #futures_to_decoded_fields
                    })})
                }
            }

            impl<GoodsAssetBuilder> ::goods::AssetContainerBuild<GoodsAssetBuilder> for #ty
            where
                #builder_bounds
            {
                fn build(mut decoded: #decoded, builder: &mut GoodsAssetBuilder) -> Result<Self, #error> {
                    ::std::result::Result::Ok(#ty {
                        #decoded_to_asset_fields
                    })
                }
            }
        },
        syn::Fields::Named(_) => unreachable!(),
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
