use {
    futures_core::future::LocalBoxFuture,
    goods::{AssetNotFound, LocalSource},
    js_sys::{ArrayBuffer, Uint8Array},
    std::{
        fmt::{self, Display},
        future::ready,
        marker::PhantomData,
    },
    wasm_bindgen::JsValue,
    wasm_bindgen_futures::JsFuture,
    web_sys::Response,
};

#[derive(Debug, Default)]
pub struct FetchSource {
    marker: PhantomData<*mut u8>,
}

impl FetchSource {
    pub fn new() -> Self {
        FetchSource {
            marker: PhantomData,
        }
    }
}

impl<U> LocalSource<U> for FetchSource
where
    U: AsRef<str>,
{
    fn read_local(&self, url: &U) -> LocalBoxFuture<'static, eyre::Result<Box<[u8]>>> {
        match web_sys::window() {
            Some(window) => {
                let future: JsFuture = window.fetch_with_str(url.as_ref()).into();

                let fut = async move {
                    match future.await {
                        Ok(response) => {
                            let response = Response::from(response);
                            if response.ok() {
                                match response.array_buffer() {
                                    Ok(promise) => match JsFuture::from(promise).await {
                                        Ok(array_buffer) => {
                                            let array_buffer = ArrayBuffer::from(array_buffer);
                                            let u8array = Uint8Array::new(&array_buffer);
                                            Ok(u8array.to_vec().into_boxed_slice())
                                        }
                                        Err(err) => Err(JsError::from(err).into()),
                                    },
                                    Err(err) => Err(JsError::from(err).into()),
                                }
                            } else {
                                #[cfg(feature = "trace")]
                                tracing::debug!(
                                    "Asset fetch failed. Status: {}",
                                    response.status()
                                );
                                Err(AssetNotFound.into())
                            }
                        }
                        Err(_err) => {
                            #[cfg(feature = "trace")]
                            tracing::debug!("Asset fetch failed. Error: {:?}", _err);
                            Err(AssetNotFound.into())
                        }
                    }
                };

                Box::pin(fut)
            }
            None => {
                #[cfg(feature = "trace")]
                tracing::error!("Failed to fetch `Window`");
                Box::pin(ready(Err(AssetNotFound.into())))
            }
        }
    }
}

#[derive(Clone, Debug)]
#[repr(transparent)]
struct JsError(String);

impl From<JsValue> for JsError {
    fn from(value: JsValue) -> Self {
        let string = match js_sys::JSON::stringify(&value) {
            Ok(string) => String::from(string),
            Err(_) => format!("<{:?}>", value),
        };
        JsError(string)
    }
}

impl Display for JsError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(&self.0)
    }
}

impl std::error::Error for JsError {}
