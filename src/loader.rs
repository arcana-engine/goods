use {
    crate::{
        asset::{Asset, Format},
        process::ProcessSlot,
        source::SourceError,
        sync::{BoxFuture, Send},
        Cache, Error, WeakCache,
    },
    alloc::{boxed::Box, vec::Vec},
    core::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
    futures_core::ready,
    pin_utils::{unsafe_pinned, unsafe_unpinned},
};

trait AnyLoaderTask<K>: Send {
    /// Drives loader task to completion.
    /// Can provoke more work by fetching new assets from provided `Cache`.
    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>, cache: &Cache<K>) -> Poll<()>;
}

struct AssetLoaderTask<A: Asset, F, D> {
    loading: BoxFuture<'static, Result<Vec<u8>, SourceError>>,
    format: Option<F>,
    decode: Option<D>,
    slot: Option<ProcessSlot<A>>,
}

impl<A, F, D> AssetLoaderTask<A, F, D>
where
    A: Asset,
{
    unsafe_unpinned!(loading: BoxFuture<'static, Result<Vec<u8>, SourceError>>);
    unsafe_unpinned!(format: Option<F>);
    unsafe_pinned!(decode: Option<D>);
    unsafe_unpinned!(slot: Option<ProcessSlot<A>>);
}

impl<A, K, F> AnyLoaderTask<K> for AssetLoaderTask<A, F, F::DecodeFuture>
where
    A: Asset,
    F: Format<A, K>,
{
    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>, cache: &Cache<K>) -> Poll<()> {
        if let Some(decode) = self.as_mut().decode().as_pin_mut() {
            match ready!(decode.poll(ctx)) {
                Ok(asset) => {
                    self.slot().take().unwrap().set(Ok(asset));
                }
                Err(err) => {
                    self.slot()
                        .take()
                        .unwrap()
                        .set(Err(Error::Asset(err.into())));
                }
            }
            return Poll::Ready(());
        }

        match ready!(self.as_mut().loading().as_mut().poll(ctx)) {
            Ok(bytes) => {
                let decode = self.as_mut().format().take().unwrap().decode(bytes, cache);
                self.as_mut().decode().set(Some(decode));

                let decode = self.as_mut().decode().as_pin_mut().unwrap();
                match ready!(decode.poll(ctx)) {
                    Ok(asset) => {
                        self.slot().take().unwrap().set(Ok(asset));
                    }
                    Err(err) => {
                        self.slot()
                            .take()
                            .unwrap()
                            .set(Err(Error::Asset(err.into())));
                    }
                }
            }
            Err(SourceError::NotFound) => self.slot().take().unwrap().set(Err(Error::NotFound)),
            Err(SourceError::Error(err)) => {
                self.slot().take().unwrap().set(Err(Error::Source(err)))
            }
        }

        Poll::Ready(())
    }
}

#[repr(transparent)]
pub(crate) struct LoaderTask<K> {
    boxed: Pin<Box<dyn AnyLoaderTask<K>>>,
}

impl<K> LoaderTask<K> {
    pub(crate) fn new<A: Asset, F: Format<A, K>>(
        loading: BoxFuture<'static, Result<Vec<u8>, SourceError>>,
        format: F,
        slot: ProcessSlot<A>,
    ) -> Self {
        LoaderTask {
            boxed: Box::pin(AssetLoaderTask {
                loading,
                format: Some(format),
                decode: None,
                slot: Some(slot),
            }),
        }
    }

    fn poll(&mut self, ctx: &mut Context<'_>, cache: &Cache<K>) -> Poll<()> {
        self.boxed.as_mut().poll(ctx, cache)
    }
}

/// Loader receives loading tasks from asset cache and drives them in the async context.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Loader<K> {
    tasks: Vec<LoaderTask<K>>,
    scratch: Vec<LoaderTask<K>>,
    cache: WeakCache<K>,
}

impl<K> Loader<K> {
    pub fn new(cache: &Cache<K>) -> Self {
        Loader {
            tasks: Vec::new(),
            scratch: Vec::new(),
            cache: cache.downgrade(),
        }
    }

    /// Drives loading tasks to completion.
    pub fn poll(&mut self, ctx: &mut Context<'_>) -> Poll<()> {
        let cache = match self.cache.upgrade() {
            Some(cache) => cache,
            None => return Poll::Ready(()), // Cannot continue when cache instance is dropped.
        };

        // Receive new tasks.
        match cache.inner.loader.poll(ctx, &mut self.scratch) {
            Poll::Pending => {
                // Poll pending tasks.
                let mut i = 0;
                while i < self.tasks.len() {
                    if let Poll::Ready(()) = self.tasks[i].poll(ctx, &cache) {
                        self.tasks.swap_remove(i);
                    } else {
                        i += 1;
                    }
                }
                Poll::Pending
            }
            Poll::Ready(()) => {
                // Poll pending tasks first.
                let mut i = 0;
                while i < self.tasks.len() {
                    if let Poll::Ready(()) = self.tasks[i].poll(ctx, &cache) {
                        self.tasks.swap_remove(i);
                    } else {
                        i += 1;
                    }
                }

                // Poll new tasks and push pending to the pending tasks list.
                for mut task in self.scratch.drain(..) {
                    if let Poll::Pending = task.poll(ctx, &cache) {
                        self.tasks.push(task);
                    }
                }
                Poll::Pending
            }
        }
    }

    /// Drives all pending loading jobs to completion.
    pub fn flush<'a>(&'a mut self) -> LoaderFlush<'a, K> {
        LoaderFlush { loader: self }
    }
}

/// Future that polls `Loader` from which it was created
/// Until no pending tasks remains.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct LoaderFlush<'a, K> {
    loader: &'a mut Loader<K>,
}

impl<'a, K> Unpin for LoaderFlush<'a, K> {}

impl<'a, K> Future for LoaderFlush<'a, K> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<()> {
        let LoaderFlush { loader } = self.get_mut();

        match loader.poll(ctx) {
            Poll::Pending => {
                if loader.tasks.is_empty() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
            Poll::Ready(()) => Poll::Ready(()),
        }
    }
}

impl<K> Future for Loader<K> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<()> {
        self.get_mut().poll(ctx)
    }
}
