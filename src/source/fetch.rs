use {
    crate::source::{LocalSource, SourceError},
    alloc::{boxed::Box, string::String, vec::Vec, sync::Arc, format},
    core::fmt::{self, Display},
    js_sys::{ArrayBuffer, Uint8Array},
    wasm_bindgen::JsValue,
    wasm_bindgen_futures::JsFuture,
    web_sys::Response,
    futures_core::future::LocalBoxFuture,
};

#[cfg_attr(all(doc, feature = "unstable-doc"), doc(cfg(feature = "fetch")))]
#[derive(Debug, Default)]
pub struct FetchSource;

impl FetchSource {
    pub fn new() -> Self {
        FetchSource
    }
}

impl<U> LocalSource<U> for FetchSource
where
    U: AsRef<str>,
{
    fn read(&self, url: &U) -> LocalBoxFuture<'_, Result<Vec<u8>, SourceError>> {
        match web_sys::window() {
            Some(window) => {
                let future: JsFuture = window.fetch_with_str(url.as_ref()).into();
                Box::pin(async move {
                    match future.await {
                        Ok(response) => {
                            let response = Response::from(response);
                            if response.ok() {
                                match response.array_buffer() {
                                    Ok(promise) => match JsFuture::from(promise).await {
                                        Ok(array_buffer) => {
                                            let array_buffer = ArrayBuffer::from(array_buffer);
                                            let u8array = Uint8Array::new(&array_buffer);
                                            Ok(u8array.to_vec())
                                        }
                                        Err(err) => Err(SourceError::Error(Arc::new(JsError::from(err)))),
                                    },
                                    Err(err) => Err(SourceError::Error(Arc::new(JsError::from(err)))),
                                }
                            } else {
                                #[cfg(feature = "trace")]
                                tracing::debug!("Asset fetch failed. Status: {}", response.status());
                                Err(SourceError::NotFound)
                            }
                        }
                        Err(_err) => {
                            #[cfg(feature = "trace")]
                            tracing::debug!("Asset fetch failed. Error: {:?}", _err);
                            Err(SourceError::NotFound)
                        }
                    }
                })
            }
            None => {
                #[cfg(feature = "trace")]
                tracing::error!("Failed to fetch `Window`");
                Box::pin(async { Err(SourceError::NotFound) })
            }
        }
    }
}

#[derive(Debug)]
struct JsError(String);

impl From<JsValue> for JsError {
    fn from(value: JsValue) -> Self {
        match js_sys::JSON::stringify(&value) {
            Ok(string) => JsError(String::from(string)),
            Err(_) => JsError(format!("<{:?}>", value)),
        }
    }
}

impl Display for JsError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(&self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for JsError {}
