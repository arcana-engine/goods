use {
    crate::{asset::Asset, Error},
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
    maybe_sync::{Mutex, Rc},
};

/// Handle for an asset of type `A` that eventually
/// resolves to the asset instance or an error.
///
/// `Handle` implements `Future` which helps with compound asset loading.
/// Unlike many `Future`s `Handle` semantically is just a pointer
/// to the place where asset isntance will be.
/// So polling `Handle` isn't necessary for asset to be loaded.
/// When asset is finally loaded any task that polled `Handle` will be notified.
pub struct Handle<A: Asset> {
    state: Rc<State<A>>,
}

impl<A: Asset> Clone for Handle<A> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
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
    wakers: Mutex<Vec<Waker>>,
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
        if *self.set.get_mut() {
            unsafe { ptr::drop_in_place({ &mut *self.storage.get() }.as_mut_ptr()) }
        }
    }
}

impl<A> Future for Handle<A>
where
    A: Asset + Clone,
{
    type Output = Result<A, Error<A>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Result<A, Error<A>>> {
        let state = &self.into_ref().get_ref().state;

        if state.set.load(Ordering::Acquire) {
            Poll::Ready(Result::clone(unsafe {
                // check above guaranties that storage was initialzied
                // and will never accessed mutably until `State` is dropped.
                &*(state.storage.get() as *const Result<A, Error<A>>)
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
                Poll::Ready(
                    unsafe {
                        // check above guaranties that storage was initialzied
                        // and will never accessed mutably until `State` is dropped.
                        &*(state.storage.get() as *const Result<A, Error<A>>)
                    }
                    .clone(),
                )
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
            state: Rc::new(State {
                wakers: Mutex::default(),
                storage: UnsafeCell::new(MaybeUninit::uninit()),
                set: AtomicBool::new(false),
            }),
        }
    }

    /// Queries for the asset state.
    /// Returns `Poll::Ready(Ok(asset))` if asset was successfully loaded.
    /// Returns `Poll::Ready(Err(error))` if error occured.
    /// Otherwise returns `Poll::Pending` as asset wasn't yet loaded.
    pub fn query(&self) -> Poll<&Result<A, Error<A>>> {
        if self.state.set.load(Ordering::Acquire) {
            Poll::Ready(unsafe { &*(self.state.storage.get() as *mut Result<A, Error<A>>) })
        } else {
            Poll::Pending
        }
    }

    /// Checks if asset referenced by this handle is not loaded yet.
    pub fn is_pending(&self) -> bool {
        !self.is_ready()
    }

    /// Checks if loading of the asset referenced by this handle is complete.
    pub fn is_ready(&self) -> bool {
        self.state.set.load(Ordering::Relaxed)
    }

    /// Checks if loading of the asset referenced by this handle failed.
    pub fn is_err(&self) -> bool {
        match self.query() {
            Poll::Pending => false,
            Poll::Ready(result) => result.is_err(),
        }
    }

    /// Checks if loading of the asset referenced by this handle succeeded.
    pub fn is_ok(&self) -> bool {
        match self.query() {
            Poll::Pending => false,
            Poll::Ready(result) => result.is_ok(),
        }
    }

    /// Returns asset instance if it's loaded.
    pub fn get(&self) -> Option<&A> {
        match self.query() {
            Poll::Ready(Ok(asset)) => Some(asset),
            _ => None,
        }
    }

    pub(crate) fn set(&self, result: Result<A, Error<A>>) {
        assert!(!self.state.set.load(Ordering::SeqCst));
        unsafe {
            ptr::write((&mut *self.state.storage.get()).as_mut_ptr(), result);
        }
        self.state.set.store(true, Ordering::Release);
        self.state.wakers.lock().drain(..).for_each(Waker::wake);
    }
}

#[derive(Clone)]
pub(crate) struct AnyHandle {
    #[cfg(not(feature = "sync"))]
    state: Rc<dyn Any>,

    #[cfg(feature = "sync")]
    state: Rc<dyn Any + Send + Sync>,
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
            state: Rc::downcast(self.state).ok()?,
        })
    }
}
