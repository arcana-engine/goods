use {
    crate::{
        asset::Asset,
        sync::{Lock, Ptr},
        Error,
    },
    alloc::vec::Vec,
    core::{
        any::Any,
        cell::UnsafeCell,
        future::Future,
        hash::{Hash, Hasher},
        mem::MaybeUninit,
        pin::Pin,
        ptr,
        sync::atomic::{AtomicBool, Ordering},
        task::{Context, Poll, Waker},
    },
};

/// Handle for an asset of type `A` that eventually
/// resolves to the asset instance or an error.
///
/// `Handle` implements `Future` which helps with compound asset loading.
/// Unlike many `Future`s `Handle` semantically is just a pointer
/// to the place where asset isntance will be.
/// So polling `Handle` isn't necessary for asset to be loaded.
/// When asset is finally loaded any task that polled `Handle` will be notified.
#[derive(Clone)]
pub struct Handle<A: Asset> {
    state: Ptr<State<A>>,
}

impl<A> Eq for Handle<A> where A: Asset {}

impl<A> PartialEq for Handle<A>
where
    A: Asset,
{
    fn eq(&self, rhs: &Self) -> bool {
        ptr::eq(&self.state, &rhs.state)
    }
}

impl<A> Hash for Handle<A>
where
    A: Asset,
{
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        ptr::hash(&self.state, state)
    }
}

struct State<A: Asset> {
    wakers: Lock<Vec<Waker>>,
    storage: UnsafeCell<MaybeUninit<Result<A, Error<A>>>>,
    set: AtomicBool,
}

unsafe impl<A> Send for State<A> where A: Asset + Send {}
unsafe impl<A> Sync for State<A> where A: Asset + Sync {}

impl<A> Drop for State<A>
where
    A: Asset,
{
    fn drop(&mut self) {
        if *self.set.get_mut() {}
    }
}

impl<A> Future for Handle<A>
where
    A: Asset,
{
    type Output = Result<A, Error<A>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Result<A, Error<A>>> {
        let state = &self.into_ref().get_ref().state;

        if state.set.load(Ordering::Acquire) {
            Poll::Ready(Result::clone(unsafe {
                &*(state.storage.get() as *mut Result<A, Error<A>>)
            }))
        } else {
            let waker = context.waker();
            let mut wakers = state.wakers.lock();
            if let Some(pos) = wakers.iter().position(|w| w.will_wake(waker)) {
                wakers[pos] = waker.clone();
            } else {
                wakers.push(waker.clone());
            }

            // Try again.
            if state.set.load(Ordering::Acquire) {
                Poll::Ready(unsafe { &*(state.storage.get() as *mut Result<A, Error<A>>) }.clone())
            } else {
                Poll::Pending
            }
        }
    }
}

impl<A> Handle<A>
where
    A: Asset,
{
    pub(crate) fn new() -> Self {
        Handle {
            state: Ptr::new(State {
                wakers: Lock::default(),
                storage: UnsafeCell::new(MaybeUninit::uninit()),
                set: AtomicBool::new(false),
            }),
        }
    }

    pub(crate) fn set(&self, result: Result<A, Error<A>>) {
        assert!(false == self.state.set.load(Ordering::SeqCst));
        unsafe {
            ptr::write((&mut *self.state.storage.get()).as_mut_ptr(), result);
        }
        self.state.set.store(true, Ordering::Release);
        self.state.wakers.lock().drain(..).for_each(Waker::wake);
    }

    /// Queries for the asset state.
    /// Returns `Poll::Ready(Ok(asset))` if asset was successfully loaded.
    /// Returns `Poll::Ready(Err(error))` if error occured.
    /// Otherwise returns `Poll::Pending` if asset wasn't yet loaded.
    pub fn query(&self) -> Poll<&Result<A, Error<A>>> {
        if self.state.set.load(Ordering::Acquire) {
            Poll::Ready(unsafe { &*(self.state.storage.get() as *mut Result<A, Error<A>>) })
        } else {
            Poll::Pending
        }
    }

    /// Returns asset instance if it's loaded.
    pub fn get(&self) -> Option<&A> {
        match self.query() {
            Poll::Ready(Ok(asset)) => Some(asset),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct AnyHandle {
    #[cfg(not(feature = "sync"))]
    state: Ptr<dyn Any>,

    #[cfg(feature = "sync")]
    state: Ptr<dyn Any + Send + Sync>,
}

impl<A> From<Handle<A>> for AnyHandle
where
    A: Asset,
{
    fn from(handle: Handle<A>) -> Self {
        AnyHandle {
            state: handle.state,
        }
    }
}

impl AnyHandle {
    pub fn downcast<A: Asset>(self) -> Option<Handle<A>> {
        Some(Handle {
            state: Ptr::downcast(self.state).ok()?,
        })
    }
}
