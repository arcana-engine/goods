use {
    crate::sync::Lock,
    alloc::vec::Vec,
    core::{
        mem::swap,
        task::{Context, Poll, Waker},
    },
};

/// Spin-lock based shareable queue.
pub(crate) struct Queue<T> {
    inner: Lock<Inner<T>>,
}

impl<T> Queue<T> {
    pub(crate) fn new() -> Self {
        let inner = Lock::new(Inner {
            array: Vec::new(),
            wakers: Vec::new(),
        });
        Queue { inner }
    }

    pub(crate) fn push(&self, value: T) {
        let mut lock = self.inner.lock();
        lock.array.push(value);
        for waker in lock.wakers.drain(..) {
            waker.wake();
        }
    }

    pub(crate) fn take(&self, scratch: &mut Vec<T>) {
        debug_assert!(scratch.is_empty());
        swap(&mut self.inner.lock().array, scratch);
    }

    pub(crate) fn poll(&self, ctx: &mut Context<'_>, scratch: &mut Vec<T>) -> Poll<()> {
        debug_assert!(scratch.is_empty());
        let mut lock = self.inner.lock();
        if lock.array.is_empty() {
            let waker = ctx.waker();
            if !lock.wakers.iter().any(|w| w.will_wake(waker)) {
                lock.wakers.push(waker.clone());
            }
            drop(lock);
            Poll::Pending
        } else {
            swap(&mut lock.array, scratch);
            Poll::Ready(())
        }
    }
}

struct Inner<T> {
    array: Vec<T>,
    wakers: Vec<Waker>,
}
