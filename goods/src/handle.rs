use {
    crate::asset::Asset,
    futures_core::future::{BoxFuture, LocalBoxFuture},
    slab::Slab,
    std::{
        any::Any,
        cell::UnsafeCell,
        error::Error,
        fmt::{self, Debug, Display},
        future::Future,
        mem::ManuallyDrop,
        pin::Pin,
        rc::Rc,
        sync::{
            atomic::{AtomicUsize, Ordering::*},
            Arc,
        },
        task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    },
};

#[cfg(all(feature = "parking_lot", not(target_arch = "wasm32")))]
use parking_lot::Mutex;

#[cfg(any(not(feature = "parking_lot"), target_arch = "wasm32"))]
use std::sync::Mutex;

#[derive(Clone)]
#[repr(transparent)]
struct SharedReport(Arc<eyre::Report>);

impl Debug for SharedReport {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&*self.0, fmt)
    }
}

impl Display for SharedReport {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&*self.0, fmt)
    }
}

impl Error for SharedReport {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

union FutureOrResult<A> {
    future: ManuallyDrop<BoxFuture<'static, Result<A, SharedReport>>>,
    result: ManuallyDrop<Result<A, SharedReport>>,
}

const WAKER_INDEX_NULL: usize = usize::MAX;
const IDLE: usize = 0;
const POLLING: usize = 1;
const POISONED: usize = 2;
const COMPLETE: usize = 3;

struct Inner<A> {
    state: AtomicUsize,
    wakers: Mutex<Slab<Option<Waker>>>,
    future_or_result: UnsafeCell<FutureOrResult<A>>,
}

impl<A> Debug for Inner<A> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Inner")
            .field("state", &self.state.load(Relaxed))
            .field("wakers", &"...")
            .field("future_or_result", &"...")
            .finish()
    }
}

/// Access to future is synchronized through .state
unsafe impl<A> Sync for Inner<A> where A: Send + Sync {}

#[derive(Clone, Debug)]
pub struct Handle<A> {
    inner: Arc<Inner<A>>,
    waker_index: usize,
}

impl<A> Handle<A>
where
    A: Asset,
{
    pub(crate) fn from_future<F>(fut: F) -> Self
    where
        F: Future<Output = eyre::Result<A>> + Send + 'static,
    {
        Handle {
            inner: Arc::new(Inner {
                state: AtomicUsize::new(IDLE),
                wakers: Mutex::new(Slab::new()),
                future_or_result: UnsafeCell::new(FutureOrResult {
                    future: ManuallyDrop::new(Box::pin(async move {
                        fut.await.map_err(|err| SharedReport(Arc::new(err)))
                    })),
                }),
            }),
            waker_index: WAKER_INDEX_NULL,
        }
    }

    pub(crate) fn erase_type(self) -> AnyHandle
    where
        A: Send + Sync + 'static,
    {
        AnyHandle { inner: self.inner }
    }

    unsafe fn result_unchecked(&self) -> eyre::Result<A> {
        match &*(*self.inner.future_or_result.get()).result {
            Ok(asset) => Ok(asset.clone()),
            Err(err) => Err(err.clone().into()),
        }
    }

    unsafe fn future_unchecked(&self) -> Pin<&mut BoxFuture<'static, Result<A, SharedReport>>> {
        Pin::new_unchecked(&mut *(*self.inner.future_or_result.get()).future)
    }

    unsafe fn resolve_unchecked(&self, result: Result<A, SharedReport>) {
        std::ptr::drop_in_place(&mut *(*self.inner.future_or_result.get()).future);
        (*self.inner.future_or_result.get()).result = ManuallyDrop::new(result);
    }

    fn raw_waker_vtable() -> &'static RawWakerVTable {
        &RawWakerVTable::new(
            |ptr| unsafe {
                let arc = Arc::from_raw(ptr as *const Inner<A>);
                std::mem::forget(arc.clone());
                let ptr = Arc::into_raw(arc);
                RawWaker::new(ptr as _, Self::raw_waker_vtable())
            },
            |ptr| unsafe {
                let inner = &*(ptr as *const Inner<A>);
                for (_, waker) in &mut *inner.wakers.lock().unwrap() {
                    if let Some(waker) = waker.take() {
                        waker.wake()
                    }
                }
            },
            |ptr| unsafe {
                let inner = &*(ptr as *const Inner<A>);
                for (_, waker) in &mut *inner.wakers.lock().unwrap() {
                    if let Some(waker) = waker.take() {
                        waker.wake()
                    }
                }
            },
            |ptr| unsafe {
                Arc::from_raw(ptr as *const Inner<A>);
            },
        )
    }

    fn raw_waker(&self) -> RawWaker {
        let ptr = Arc::into_raw(self.inner.clone());
        RawWaker::new(ptr as _, Self::raw_waker_vtable())
    }

    fn waker(&self) -> Waker {
        unsafe { Waker::from_raw(self.raw_waker()) }
    }
}

impl<A> Future for Handle<A>
where
    A: Asset,
{
    type Output = eyre::Result<A>;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<eyre::Result<A>> {
        if self.inner.state.load(Acquire) == COMPLETE {
            return Poll::Ready(unsafe { self.result_unchecked() });
        }

        let waker = ctx.waker().clone();

        {
            let wakers = self.inner.wakers.lock();
            #[cfg(all(feature = "parking_lot", not(target_arch = "wasm32")))]
            let mut wakers = wakers;
            #[cfg(any(not(feature = "parking_lot"), target_arch = "wasm32"))]
            let mut wakers = wakers.unwrap();

            if self.waker_index == WAKER_INDEX_NULL {
                let waker_index = wakers.insert(Some(waker));
                drop(wakers);
                self.waker_index = waker_index;
            } else {
                wakers[self.waker_index] = Some(waker);
            }
        }

        match self
            .inner
            .state
            .compare_exchange(IDLE, POLLING, Acquire, Acquire)
        {
            Ok(IDLE) => {}
            Err(POLLING) => return Poll::Pending,
            Err(COMPLETE) => return Poll::Ready(unsafe { self.result_unchecked() }),
            Err(POISONED) => panic!("Future paniced during poll"),
            _ => unsafe { std::hint::unreachable_unchecked() },
        }

        struct Poison<'a>(&'a AtomicUsize);

        impl Drop for Poison<'_> {
            fn drop(&mut self) {
                self.0.store(POISONED, Release);
            }
        }

        let waker = self.waker();
        let mut ctx = Context::from_waker(&waker);

        unsafe {
            let poison = Poison(&self.inner.state);
            let poll = self.future_unchecked().poll(&mut ctx);
            std::mem::forget(poison);

            match poll {
                Poll::Pending => {
                    self.inner.state.store(IDLE, Release);
                    Poll::Pending
                }
                Poll::Ready(result) => {
                    self.resolve_unchecked(result);
                    self.inner.state.store(COMPLETE, Release);

                    // Wake everyone
                    let mut wakers = {
                        let wakers = self.inner.wakers.lock();
                        #[cfg(all(feature = "parking_lot", not(target_arch = "wasm32")))]
                        let mut wakers = wakers;
                        #[cfg(any(not(feature = "parking_lot"), target_arch = "wasm32"))]
                        let mut wakers = wakers.unwrap();

                        std::mem::replace(&mut *wakers, Slab::new())
                    };

                    for waker in wakers.drain() {
                        if let Some(waker) = waker {
                            waker.wake()
                        }
                    }

                    Poll::Ready(self.result_unchecked())
                }
            }
        }
    }
}

pub(crate) struct AnyHandle {
    inner: Arc<dyn Any + Send + Sync>,
}

impl AnyHandle {
    pub unsafe fn downcast<A: Asset>(&self) -> Handle<A> {
        debug_assert!(Any::is::<Handle<A>>(&self.inner));

        let ptr: *const Handle<A> = &self.inner as *const dyn Any as _;
        Clone::clone(&*ptr)
    }
}

enum LocalFutureOrResult<A> {
    Future(LocalBoxFuture<'static, Result<A, SharedReport>>),
    Result(Result<A, SharedReport>),
    Poisoned,
}

impl<A> Debug for LocalFutureOrResult<A>
where
    A: Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Future(_) => write!(fmt, "Future<{}>", std::any::type_name::<A>()),
            Self::Result(result) => Debug::fmt(result, fmt),
            Self::Poisoned => fmt.write_str("Poisoned"),
        }
    }
}

#[derive(Debug)]
struct LocalInner<A> {
    wakers: UnsafeCell<Slab<Waker>>,
    future_or_result: UnsafeCell<LocalFutureOrResult<A>>,
}

#[derive(Clone, Debug)]
pub struct LocalHandle<A> {
    inner: Rc<LocalInner<A>>,
    waker_index: usize,
}

impl<A> LocalHandle<A>
where
    A: Asset,
{
    pub(crate) fn from_future<F>(fut: F) -> Self
    where
        F: Future<Output = eyre::Result<A>> + 'static,
    {
        LocalHandle {
            inner: Rc::new(LocalInner {
                wakers: UnsafeCell::new(Slab::new()),
                future_or_result: UnsafeCell::new(LocalFutureOrResult::Future(Box::pin(
                    async move { fut.await.map_err(|err| SharedReport(Arc::new(err))) },
                ))),
            }),
            waker_index: WAKER_INDEX_NULL,
        }
    }

    pub(crate) fn erase_type(self) -> AnyLocalHandle
    where
        A: 'static,
    {
        AnyLocalHandle { inner: self.inner }
    }

    unsafe fn result_unchecked(&self) -> eyre::Result<A> {
        match &*self.inner.future_or_result.get() {
            LocalFutureOrResult::Future(_) | LocalFutureOrResult::Poisoned => {
                std::hint::unreachable_unchecked()
            }
            LocalFutureOrResult::Result(Ok(asset)) => Ok(asset.clone()),
            LocalFutureOrResult::Result(Err(err)) => Err(err.clone().into()),
        }
    }

    unsafe fn resolve_unchecked(&self, result: Result<A, SharedReport>) {
        match &mut *self.inner.future_or_result.get() {
            slot @ LocalFutureOrResult::Future(_) => {
                *slot = LocalFutureOrResult::Result(result);
            }
            LocalFutureOrResult::Result(_) | LocalFutureOrResult::Poisoned => {
                std::hint::unreachable_unchecked()
            }
        }
    }
}

impl<A> Future for LocalHandle<A>
where
    A: Asset,
{
    type Output = eyre::Result<A>;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<eyre::Result<A>> {
        struct Poison<'a, A>(&'a UnsafeCell<LocalFutureOrResult<A>>);

        impl<A> Drop for Poison<'_, A> {
            fn drop(&mut self) {
                unsafe {
                    *self.0.get() = LocalFutureOrResult::Poisoned;
                }
            }
        }

        unsafe {
            match &mut *self.inner.future_or_result.get() {
                LocalFutureOrResult::Future(fut) => {
                    let poison = Poison(&self.inner.future_or_result);
                    let poll = Pin::new_unchecked(fut).poll(ctx);
                    std::mem::forget(poison);
                    match poll {
                        Poll::Pending => {
                            let waker = ctx.waker().clone();
                            if self.waker_index == WAKER_INDEX_NULL {
                                self.waker_index = (&mut *self.inner.wakers.get()).insert(waker);
                            } else {
                                (&mut *self.inner.wakers.get())[self.waker_index] = waker;
                            }
                            Poll::Pending
                        }
                        Poll::Ready(result) => {
                            self.resolve_unchecked(result);
                            for waker in (&mut *self.inner.wakers.get()).drain() {
                                waker.wake();
                            }

                            Poll::Ready(self.result_unchecked())
                        }
                    }
                }
                LocalFutureOrResult::Result(Ok(asset)) => Poll::Ready(Ok(asset.clone())),
                LocalFutureOrResult::Result(Err(err)) => Poll::Ready(Err(err.clone().into())),
                LocalFutureOrResult::Poisoned => panic!("Future paniced during poll"),
            }
        }
    }
}

pub(crate) struct AnyLocalHandle {
    inner: Rc<dyn Any>,
}

impl AnyLocalHandle {
    pub unsafe fn downcast<A: Asset>(&self) -> LocalHandle<A> {
        debug_assert!(Any::is::<LocalHandle<A>>(&self.inner));

        let ptr: *const LocalHandle<A> = &self.inner as *const dyn Any as _;
        Clone::clone(&*ptr)
    }
}
