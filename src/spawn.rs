use futures_core::future::{BoxFuture, LocalBoxFuture};

pub struct SpawnError;

pub trait Spawn: core::fmt::Debug {
    fn spawn(&self, future: BoxFuture<'static, ()>) -> Result<(), SpawnError>;
}

pub trait LocalSpawn: core::fmt::Debug {
    fn spawn(&self, future: LocalBoxFuture<'static, ()>) -> Result<(), SpawnError>;
}

#[cfg(all(feature = "futures-spawn"))]
impl<S> Spawn for S
where
    S: futures_task::Spawn + core::fmt::Debug,
{
    fn spawn(&self, future: BoxFuture<'static, ()>) -> Result<(), SpawnError> {
        <S as futures_util::task::SpawnExt>::spawn(self, future).map_err(|_| SpawnError)
    }
}

#[cfg(all(feature = "futures-spawn"))]
impl<S> LocalSpawn for S
where
    S: futures_task::LocalSpawn + core::fmt::Debug,
{
    fn spawn(&self, future: LocalBoxFuture<'static, ()>) -> Result<(), SpawnError> {
        <S as futures_util::task::LocalSpawnExt>::spawn_local(self, future).map_err(|_| SpawnError)
    }
}

#[cfg(feature = "tokio-spawn")]
#[derive(Clone, Debug)]
#[cfg_attr(all(doc, feature = "unstable-doc"), doc(cfg(feature = "tokio-spawn")))]
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
#[cfg_attr(
    all(doc, feature = "unstable-doc"),
    doc(all(feature = "wasm-bindgen-spawn", target_arch = "wasm32"))
)]
pub struct WasmBindgen;

#[cfg(all(feature = "wasm-bindgen-spawn", target_arch = "wasm32"))]
impl LocalSpawn for WasmBindgen {
    fn spawn(&self, future: LocalBoxFuture<'static, ()>) -> Result<(), SpawnError> {
        wasm_bindgen_futures::spawn_local(future);
        Ok(())
    }
}
