use {
    crate::sync::{Lock, Ptr},
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
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            inner: self.inner.clone(),
        }
    }
}

pub(crate) struct Receiver<T> {
    received: Vec<T>,
    inner: Ptr<Lock<Inner<T>>>,
}

struct Inner<T> {
    array: Vec<T>,
    waker: Option<Waker>,
}

impl<T> Receiver<T> {
    // pub(crate) fn recv(&mut self) -> Option<T> {
    //     if let Some(value) = self.received.pop() {
    //         Some(value)
    //     } else {
    //         let mut lock = self.inner.lock();
    //         swap(&mut lock.array, &mut self.received);
    //         drop(lock);
    //         self.received.pop()
    //     }
    // }

    pub(crate) fn recv_batch(&mut self) -> Vec<T> {
        let mut scratch = Vec::new();
        let mut lock = self.inner.lock();
        swap(&mut lock.array, &mut scratch);
        drop(lock);
        scratch.append(&mut self.received);
        scratch
    }

    pub(crate) fn poll(&mut self, ctx: &mut Context<'_>) -> Poll<Option<T>> {
        if let Some(value) = self.received.pop() {
            Poll::Ready(Some(value))
        } else if let Some(inner) = Ptr::get_mut(&mut self.inner) {
            let mut lock = inner.lock();
            if lock.array.is_empty() {
                drop(lock);
                Poll::Ready(None)
            } else {
                swap(&mut lock.array, &mut self.received);
                Poll::Ready(self.received.pop())
            }
        } else {
            let mut lock = self.inner.lock();
            if lock.array.is_empty() {
                lock.waker = Some(ctx.waker().clone());
                drop(lock);
                Poll::Pending
            } else {
                swap(&mut lock.array, &mut self.received);
                Poll::Ready(self.received.pop())
            }
        }
    }
}

pub(crate) fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Ptr::new(Lock::new(Inner {
        array: Vec::new(),
        waker: None,
    }));
    let receiver = Receiver {
        received: Vec::new(),
        inner: inner.clone(),
    };
    let sender = Sender { inner };
    (sender, receiver)
}
