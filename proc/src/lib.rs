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
#[proc_macro_derive(Asset, attributes(external, container, serde))]
pub fn asset(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match parse(item) {
        Ok(parsed) => asset_impl(parsed),
        Err(error) => error.into_compile_error(),
    }
    .into()
}

#[proc_macro_derive(AssetField, attributes(external, container, serde))]
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
}

fn parse(item: proc_macro::TokenStream) -> syn::Result<Parsed> {
    use syn::spanned::Spanned;

    let derive_input = syn::parse::<syn::DeriveInput>(item)?;

    let serde_attributes = derive_input
        .attrs
        .iter()
        .filter(|attr| {
            attr.path
                .get_ident()
                .map_or(false, |ident| ident == "serde")
        })
        .cloned()
        .collect();

    let mut decode_field_errors = proc_macro2::TokenStream::new();
    let mut build_field_errors = proc_macro2::TokenStream::new();
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
                if attr
                    .path
                    .get_ident()
                    .map_or(false, |ident| ident == "external" || ident == "container")
                {
                    Some(index)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let serde_attributes = field.attrs.iter().filter(|attr| {
            attr.path
                .get_ident()
                .map_or(false, |ident| ident == "serde")
        });

        let ty = &field.ty;

        match asset_attributes.len() {
            0 => match &field.ident {
                Some(ident) => {
                    info_fields.extend(quote::quote!(
                        #(#serde_attributes)*
                        #ident: #ty,
                    ));
                    futures_fields.extend(quote::quote!(#ident: #ty,));
                    decoded_fields.extend(quote::quote!(#ident: #ty,));
                    info_to_futures_fields.extend(quote::quote!(#ident: info.#ident,));
                    futures_to_decoded_fields.extend(quote::quote!(#ident: futures.#ident,));
                    decoded_to_asset_fields.extend(quote::quote!(#ident: decoded.#ident,));
                }
                None => {
                    info_fields.extend(quote::quote!(
                        #(#serde_attributes)*
                        #ty,
                    ));
                    futures_fields.extend(quote::quote!(#ty,));
                    decoded_fields.extend(quote::quote!(#ty,));
                    info_to_futures_fields.extend(quote::quote!(info.#index,));
                    futures_to_decoded_fields.extend(quote::quote!(futures.#index,));
                    decoded_to_asset_fields.extend(quote::quote!(decoded.#index,));
                }
            },
            1 => {
                complex = true;

                let attribute = &field.attrs[asset_attributes[0]];

                let kind = match attribute.path.get_ident().unwrap() {
                    i if i == "external" => quote::quote!(::goods::External),
                    i if i == "container" => quote::quote!(::goods::Container),
                    _ => unreachable!(),
                };

                let as_type_arg = match attribute.tokens.is_empty() {
                    true => None,
                    false => Some(attribute.parse_args_with(
                        |stream: syn::parse::ParseStream| {
                            let _as = stream.parse::<syn::Token![as]>()?;
                            let as_type = stream.parse::<syn::Type>()?;
                            Ok(as_type)
                        },
                    )?),
                };

                let as_type = as_type_arg.as_ref().unwrap_or(ty);

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
                            quote::quote!(#ident: <#as_type as ::goods::AssetField<#kind>>::Info,),
                        );
                        futures_fields.extend(
                            quote::quote!(#ident: <#as_type as ::goods::AssetField<#kind>>::Fut,),
                        );
                        decoded_fields
                            .extend(quote::quote!(#ident: <#as_type as ::goods::AssetField<#kind>>::Decoded,));
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
                        info_fields
                            .extend(quote::quote!(<#as_type as ::goods::AssetField<#kind>>::Info,));
                        futures_fields
                            .extend(quote::quote!(<#as_type as ::goods::AssetField<#kind>>::Fut,));
                        decoded_fields.extend(
                            quote::quote!(<#as_type as ::goods::AssetField<#kind>>::Decoded,),
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
    })
}

fn asset_impl(parsed: Parsed) -> proc_macro2::TokenStream {
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
    } = parsed;

    let data_struct = match &derive_input.data {
        syn::Data::Struct(data) => data,
        _ => unreachable!(),
    };

    let ty = &derive_input.ident;

    let tokens = match data_struct.fields {
        syn::Fields::Unit => quote::quote! {
            #[derive(::goods::serde::Deserialize)]
            #(#serde_attributes)*
            pub struct #info;

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #decode_error {
                #[error("Failed to deserialize asset from json")]
                Json(#[source] ::goods::serde_json::Error),

                #[error("Failed to deserialize asset from bincode")]
                Bincode(#[source] ::goods::bincode::Error),
            }

            pub type #build_error = ::std::convert::Infallible;

            impl ::goods::Asset for #ty {
                type BuildError = #build_error;
                type DecodeError = #decode_error;
                type Decoded = #info;
                type Fut = ::std::future::Ready<::std::result::Result<#info, #decode_error>>;

                fn decode(bytes: ::std::boxed::Box<[u8]>, _loader: &::goods::Loader) -> Self::Fut {
                    use {::std::result::Result::{Ok, Err}, ::goods::serde_json::error::Category};

                    /// Zero-length is definitely bincode. For unit structs we may skip deserialization.
                    if bytes.is_empty() {
                        return ready(Ok(#info))
                    }

                    let result = match ::goods::serde_json::from_slice(&*bytes) {
                        Ok(value) => Ok(value),
                        Err(err) => match err.classify() {
                            Category::Syntax => {
                                // That's not json. Bincode then.
                                match ::goods::bincode::deserialize(&*bytes) {
                                    Ok(value) => Ok(value),
                                    Err(err) => Err(#decode_error::Bincode(err)),
                                }
                            }
                            _ => {
                                Err(#decode_error::Json(err))
                            }
                        }
                    };

                    ::std::future::ready(result)
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetBuild<BuilderGenericParameter> for #ty {
                fn build(#info: #info, _builder: &mut BuilderGenericParameter) -> ::std::result::Result<Self, #build_error> {
                    Ok(#ty)
                }
            }
        },
        syn::Fields::Unnamed(_) => todo!("Not yet implemented"),
        syn::Fields::Named(_) if complex => quote::quote! {
            #[derive(::goods::serde::Deserialize)]
            #(#serde_attributes)*
            struct #info { #info_fields }

            struct #futures { #futures_fields }

            pub struct #decoded { #decoded_fields }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #decode_error {
                #[error("Failed to deserialize asset info from json")]
                Json(#[source] ::goods::serde_json::Error),

                #[error("Failed to deserialize asset info from bincode")]
                Bincode(#[source] ::goods::bincode::Error),

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
                type Fut = ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = Result<#decoded, #decode_error>> + Send>>;

                fn decode(bytes: ::std::boxed::Box<[u8]>, loader: &::goods::Loader) -> Self::Fut {
                    use {::std::{boxed::Box, result::Result::{self, Ok, Err}}, ::goods::serde_json::error::Category};

                    // Zero-length is definitely bincode.
                    let result: Result<#info, #decode_error> = if bytes.is_empty()  {
                        match ::goods::bincode::deserialize(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => Err(#decode_error::Bincode(err)),
                        }
                    } else {
                        match ::goods::serde_json::from_slice(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => match err.classify() {
                                Category::Syntax => {
                                    // That's not json. Bincode then.
                                    match ::goods::bincode::deserialize(&*bytes) {
                                        Ok(value) => Ok(value),
                                        Err(err) => Err(#decode_error::Bincode(err)),
                                    }
                                }
                                _ => {
                                    Err(#decode_error::Json(err))
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
        },
        syn::Fields::Named(_) => quote::quote! {
            #[derive(::goods::serde::Deserialize)]
            #(#serde_attributes)*
            pub struct #info { #info_fields }

            #[derive(::std::fmt::Debug, ::goods::thiserror::Error)]
            pub enum #decode_error {
                #[error("Failed to deserialize asset info from json")]
                Json(#[source] ::goods::serde_json::Error),

                #[error("Failed to deserialize asset info from bincode")]
                Bincode(#[source] ::goods::bincode::Error),
            }

            pub type #build_error = ::std::convert::Infallible;

            impl ::goods::Asset for #ty {
                type BuildError = #build_error;
                type DecodeError = #decode_error;
                type Decoded = #info;
                type Fut = ::std::future::Ready<Result<#info, #decode_error>>;

                fn decode(bytes: ::std::boxed::Box<[u8]>, _loader: &::goods::Loader) -> Self::Fut {
                    use {::std::result::Result::{Ok, Err}, ::goods::serde_json::error::Category};

                    /// Zero-length is definitely bincode.
                    let result = if bytes.is_empty()  {
                        match ::goods::bincode::deserialize(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => Err(#decode_error::Bincode(err)),
                        }
                    } else {
                        match ::goods::serde_json::from_slice(&*bytes) {
                            Ok(value) => Ok(value),
                            Err(err) => match err.classify() {
                                Category::Syntax => {
                                    // That's not json. Bincode then.
                                    match ::goods::bincode::deserialize(&*bytes) {
                                        Ok(value) => Ok(value),
                                        Err(err) => Err(#decode_error::Bincode(err)),
                                    }
                                }
                                _ => {
                                    Err(#decode_error::Json(err))
                                }
                            }
                        }
                    };

                    ::std::future::ready(result)
                }
            }

            impl<BuilderGenericParameter> ::goods::AssetBuild<BuilderGenericParameter> for #ty {
                fn build(decoded: #info, _builder: &mut BuilderGenericParameter) -> Result<Self, #build_error> {
                    Ok(#ty {
                        #decoded_to_asset_fields
                    })
                }
            }
        },
    };

    tokens
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
            #(#serde_attributes)*
            pub struct #info { #info_fields }

            struct #futures { #futures_fields }

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
