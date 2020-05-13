use {
    crate::{
        asset::{Asset, Format},
        channel::{channel, Receiver, Sender},
        process::ProcessSlot,
        source::SourceError,
        Cache, Error,
    },
    alloc::{boxed::Box, vec::Vec},
    core::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
    futures_core::{future::BoxFuture, ready},
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
pub struct Loader<K> {
    tasks: Vec<LoaderTask<K>>,
    receiver: Receiver<LoaderTask<K>>,
    sender: Sender<LoaderTask<K>>,
}

impl<K> Loader<K> {
    pub fn new() -> Self {
        let (sender, receiver) = channel();

        Loader {
            tasks: Vec::new(),
            receiver,
            sender,
        }
    }

    /// Drives loading jobs.
    pub fn poll(&mut self, ctx: &mut Context<'_>, cache: &Cache<K>) -> Poll<()> {
        // Poll tasks.
        let mut i = 0;
        while i < self.tasks.len() {
            if let Poll::Ready(()) = self.tasks[i].poll(ctx, cache) {
                self.tasks.swap_remove(i);
            } else {
                i += 1;
            }
        }

        loop {
            match self.receiver.poll(ctx) {
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(mut task)) => {
                    if let Poll::Pending = task.poll(ctx, cache) {
                        self.tasks.push(task);
                    }
                }
            }
        }
    }

    /// Drives all pending loading jobs to completion.
    pub fn flush<'a>(&'a mut self, cache: &'a Cache<K>) -> LoaderFlush<'a, K> {
        LoaderFlush {
            loader: self,
            cache,
        }
    }

    /// Drives all loading jobs including new jobs to completion.
    pub fn run<'a>(&'a mut self, cache: &'a Cache<K>) -> LoaderRun<'a, K> {
        LoaderRun {
            loader: self,
            cache,
        }
    }

    pub(crate) fn sender(&self) -> Sender<LoaderTask<K>> {
        self.sender.clone()
    }
}

/// Future that polls `Loader` from which it was created
/// Until no tasks remains.
///
/// Note that new tasks may be enqued.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct LoaderFlush<'a, K> {
    loader: &'a mut Loader<K>,
    cache: &'a Cache<K>,
}

impl<'a, K> Unpin for LoaderFlush<'a, K> {}

impl<'a, K> Future for LoaderFlush<'a, K> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<()> {
        let LoaderFlush { loader, cache } = self.get_mut();
        let _ = loader.poll(ctx, cache);
        if loader.tasks.is_empty() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

/// Future that polls `Loader` from which it was created
/// Until `Loader` is disconnected from all `Cache`s.
///
/// Note that new tasks may be enqued.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct LoaderRun<'a, K> {
    loader: &'a mut Loader<K>,
    cache: &'a Cache<K>,
}

impl<'a, K> Unpin for LoaderRun<'a, K> {}

impl<'a, K> Future for LoaderRun<'a, K> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<()> {
        let LoaderRun { loader, cache } = self.get_mut();
        loader.poll(ctx, cache)
    }
}
