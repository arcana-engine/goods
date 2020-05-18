#[cfg(feature = "sync")]
core::compile_error!("`FetchSource` cannot be used with `sync` feature which is enabled by default. If you build this crate to run in browser you may simply turn it off as there are no threads");

#[cfg(not(feature = "sync"))]
use {
    crate::{
        source::{Source, SourceError},
        sync::{BoxFuture, Ptr},
    },
    alloc::{boxed::Box, string::String, vec::Vec},
    core::fmt::{self, Display},
    js_sys::{ArrayBuffer, Uint8Array},
    wasm_bindgen::JsValue,
    wasm_bindgen_futures::JsFuture,
    web_sys::Response,
};

#[cfg(not(feature = "sync"))]
#[cfg_attr(doc, doc(cfg(feature = "fetch")))]
#[derive(Debug)]
pub struct FetchSource;

#[cfg(not(feature = "sync"))]
impl FetchSource {
    pub fn new() -> Self {
        FetchSource
    }
}

#[cfg(not(feature = "sync"))]
impl<U> Source<U> for FetchSource
where
    U: AsRef<str>,
{
    fn read(&self, url: &U) -> BoxFuture<'_, Result<Vec<u8>, SourceError>> {
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
                                        Err(err) => Err(SourceError::Error(Ptr::new(JsError(err)))),
                                    },
                                    Err(err) => Err(SourceError::Error(Ptr::new(JsError(err)))),
                                }
                            } else {
                                log::debug!("Asset fetch failed. Status: {}", response.status());
                                Err(SourceError::NotFound)
                            }
                        }
                        Err(err) => {
                            log::debug!("Asset fetch failed. Error: {:?}", err);
                            Err(SourceError::NotFound)
                        }
                    }
                })
            }
            None => {
                log::error!("Failed to fetch `Window`");
                Box::pin(async { Err(SourceError::NotFound) })
            }
        }
    }
}

#[cfg(not(feature = "sync"))]
#[derive(Clone, Debug)]
struct JsError(JsValue);

#[cfg(not(feature = "sync"))]
impl Display for JsError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match js_sys::JSON::stringify(&self.0) {
            Ok(string) => write!(fmt, "{}", String::from(string)),
            Err(_) => write!(fmt, "<{:?}>", self.0),
        }
    }
}

#[cfg(all(not(feature = "sync"), feature = "std"))]
impl std::error::Error for JsError {}
