use crate::sync::BoxFuture;

pub struct SpawnError;

pub trait Spawn: core::fmt::Debug {
    fn spawn(&self, future: BoxFuture<'static, ()>) -> Result<(), SpawnError>;
}

#[cfg(feature = "futures-spawn")]
impl<S> Spawn for S
where
    S: futures_task::Spawn + core::fmt::Debug,
{
    fn spawn(&self, future: BoxFuture<'static, ()>) -> Result<(), SpawnError> {
        <&S as futures_util::task::SpawnExt>::spawn(&self, future).map_err(|_| SpawnError)
    }
}

#[cfg(feature = "tokio-spawn")]
#[derive(Clone, Debug)]
pub struct Tokio(pub tokio::runtime::Handle);

#[cfg(feature = "tokio-spawn")]
impl Tokio {
    pub fn current() -> Self {
        Tokio(tokio::runtime::Handle::current())
    }
}

#[cfg(feature = "tokio-spawn")]
impl Spawn for Tokio {
    fn spawn(&self, future: BoxFuture<'static, ()>) -> Result<(), SpawnError> {
        self.0.spawn(future);
        Ok(())
    }
}

#[cfg(all(feature = "wasm-bindgen-spawn", target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct WasmBindgen;

#[cfg(all(feature = "wasm-bindgen-spawn", target_arch = "wasm32"))]
impl Spawn for WasmBindgen {
    fn spawn(&self, future: BoxFuture<'static, ()>) -> Result<(), SpawnError> {
        wasm_bindgen_futures::spawn_local(future);
        Ok(())
    }
}
