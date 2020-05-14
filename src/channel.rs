use {
    crate::sync::{Lock, Ptr, WeakPtr},
    alloc::vec::Vec,
    core::{
        mem::swap,
        task::{Context, Poll, Waker},
    },
};

/// Sender for spin-lock based channel.
pub(crate) struct Sender<T> {
    inner: Ptr<Lock<Inner<T>>>,
}

impl<T> Sender<T> {
    pub(crate) fn send(&self, value: T) {
        let mut lock = self.inner.lock();
        lock.array.push(value);
        if let Some(waker) = lock.waker.take() {
            waker.wake();
        }
    }

    pub(crate) fn downgrade(&self) -> WeakSender<T> {
        WeakSender {
            inner: Ptr::downgrade(&self.inner),
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            inner: self.inner.clone(),
        }
    }
}

/// Sender for spin-lock based channel.
pub(crate) struct WeakSender<T> {
    inner: WeakPtr<Lock<Inner<T>>>,
}

impl<T> WeakSender<T> {
    pub(crate) fn upgrade(&self) -> Option<Sender<T>> {
        Sender {
            inner: self.inner.upgrade()?,
        }
        .into()
    }
}

impl<T> Clone for WeakSender<T> {
    fn clone(&self) -> Self {
        WeakSender {
            inner: self.inner.clone(),
        }
    }
}

pub(crate) struct Receiver<T> {
    inner: Ptr<Lock<Inner<T>>>,
}

struct Inner<T> {
    array: Vec<T>,
    waker: Option<Waker>,
}

impl<T> Receiver<T> {
    pub(crate) fn recv(&self, scratch: &mut Vec<T>) {
        debug_assert!(scratch.is_empty());
        swap(&mut self.inner.lock().array, scratch);
    }

    pub(crate) fn poll(&mut self, ctx: &mut Context<'_>, scratch: &mut Vec<T>) -> Poll<bool> {
        debug_assert!(scratch.is_empty());
        let strong_count = Ptr::strong_count(&self.inner);
        let mut lock = self.inner.lock();
        if lock.array.is_empty() {
            if strong_count == 1 {
                Poll::Ready(false)
            } else {
                lock.waker = Some(ctx.waker().clone());
                drop(lock);
                Poll::Pending
            }
        } else {
            swap(&mut lock.array, scratch);
            Poll::Ready(true)
        }
    }
}

pub(crate) fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Ptr::new(Lock::new(Inner {
        array: Vec::new(),
        waker: None,
    }));
    let receiver = Receiver {
        inner: inner.clone(),
    };
    let sender = Sender { inner };
    (sender, receiver)
}
