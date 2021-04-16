use {
    crate::asset::Asset,
    hashbrown::hash_map::{Entry, HashMap},
    std::{
        any::{Any, TypeId},
        cell::{RefCell, UnsafeCell},
        future::Future,
        pin::Pin,
        sync::{
            atomic::{AtomicUsize, Ordering::*},
            Arc, Mutex, Weak,
        },
        task::{Context, Poll, Waker},
    },
};

trait ProcessTrait<T> {
    unsafe fn run(&self, ctx: &mut T);
}

const IDLE: usize = 0;
const POLLING: usize = 1;
const COMPLETE: usize = 2;

struct Inner<A: Asset> {
    state: AtomicUsize,
    repr: UnsafeCell<Option<A::Repr>>,
    result: UnsafeCell<Option<A::BuildFuture>>,
    waker: UnsafeCell<Option<Waker>>,
}

unsafe impl<A: Asset> Send for Inner<A>
where
    A: Asset,
    A::Repr: Send,
    A::BuildFuture: Send,
{
}

unsafe impl<A: Asset> Sync for Inner<A>
where
    A: Asset,
    A::Repr: Send,
    A::BuildFuture: Send,
{
}

impl<A> ProcessTrait<A::Context> for Inner<A>
where
    A: Asset,
{
    unsafe fn run(&self, ctx: &mut A::Context) {
        let repr = (&mut *self.repr.get()).take().unwrap();
        let fut = A::build(repr, ctx);
        *self.result.get() = Some(fut);
        match self.state.swap(COMPLETE, Release) {
            IDLE => {
                if let Some(waker) = (&mut *self.waker.get()).take() {
                    waker.wake()
                }
            }
            POLLING => {
                // Process future is polled.
                // The `waker` field may be accessed.
                // After waker is set `state` will be checked again.
            }
            _ => {
                // unreachable
            }
        }
    }
}

struct Process<T>(Weak<dyn ProcessTrait<T> + Send + Sync>);

impl<T> Process<T> {
    fn new<A: Asset<Context = T>>(repr: A::Repr) -> (Process<T>, ProcessFuture<A>)
    where
        A::Repr: Send,
        A::BuildFuture: Send,
    {
        let inner = Arc::new(Inner {
            state: AtomicUsize::new(IDLE),
            repr: UnsafeCell::new(Some(repr)),
            result: UnsafeCell::new(None),
            waker: UnsafeCell::new(None),
        });
        let weak = Arc::downgrade(&inner);
        let process = Process(weak);
        let fut = ProcessFuture(inner);
        (process, fut)
    }

    fn run(self, ctx: &mut T) {
        if let Some(p) = self.0.upgrade() {
            unsafe { p.run(ctx) }
        }
    }
}

#[repr(transparent)]
pub(crate) struct ProcessFuture<A: Asset>(Arc<Inner<A>>);

impl<A> Future for ProcessFuture<A>
where
    A: Asset,
{
    type Output = A::BuildFuture;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<A::BuildFuture> {
        let me = &*self.0;

        match me.state.compare_exchange(IDLE, POLLING, Acquire, Acquire) {
            Ok(IDLE) => {
                unsafe {
                    *me.waker.get() = Some(ctx.waker().clone());
                }

                match me.state.compare_exchange(POLLING, IDLE, AcqRel, Acquire) {
                    Err(IDLE) => {
                        unreachable!()
                    }
                    Ok(POLLING) => Poll::Pending,
                    Err(COMPLETE) => {
                        Poll::Ready(unsafe { (&mut *me.result.get()).take().unwrap() })
                    }
                    _ => unreachable!(),
                }
            }
            Err(POLLING) => {
                unreachable!()
            }
            Err(COMPLETE) => Poll::Ready(unsafe { (&mut *me.result.get()).take().unwrap() }),
            _ => unreachable!(),
        }
    }
}

struct Queue<T> {
    processes: Vec<Process<T>>,
}

pub(crate) struct Processor {
    queues: Mutex<HashMap<TypeId, Box<dyn Any + Send>>>,
}

impl Processor {
    pub fn new() -> Self {
        Processor {
            queues: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_context<A>(&self, repr: A::Repr) -> ProcessFuture<A>
    where
        A: Asset,
        A::Repr: Send,
        A::BuildFuture: Send,
    {
        let context_type_id = TypeId::of::<A::Context>();
        let (process, fut) = Process::new::<A>(repr);

        let mut lock = self.queues.lock().unwrap();
        match lock.entry(context_type_id) {
            Entry::Occupied(mut entry) => {
                let queue = entry.get_mut();
                debug_assert!(<dyn Any>::is::<Queue<A::Context>>(&**queue));
                let queue =
                    unsafe { &mut *(&mut **queue as *mut dyn Any as *mut Queue<A::Context>) };
                queue.processes.push(process);
            }
            Entry::Vacant(entry) => {
                let queue = Queue {
                    processes: vec![process],
                };
                entry.insert(Box::new(queue));
            }
        }
        drop(lock);

        fut
    }

    pub fn run<T: 'static>(&self, ctx: &mut T) {
        let context_type_id = TypeId::of::<T>();

        let mut lock = self.queues.lock().unwrap();
        if let Some(queue) = lock.get_mut(&context_type_id) {
            debug_assert!(<dyn Any>::is::<Queue<T>>(&**queue));

            let queue = unsafe { &mut *(&mut **queue as *mut dyn Any as *mut Queue<T>) };
            let processes = std::mem::take(&mut queue.processes);
            drop(lock);

            for process in processes {
                process.run(ctx);
            }
        }
    }
}

struct LocalProcess<T>(Weak<dyn ProcessTrait<T>>);

impl<T> LocalProcess<T> {
    fn new<A: Asset<Context = T>>(repr: A::Repr) -> (LocalProcess<T>, LocalProcessFuture<A>) {
        let inner = Arc::new(Inner {
            state: AtomicUsize::new(IDLE),
            repr: UnsafeCell::new(Some(repr)),
            result: UnsafeCell::new(None),
            waker: UnsafeCell::new(None),
        });
        let weak = Arc::downgrade(&inner);
        let process = LocalProcess(weak);
        let fut = LocalProcessFuture(inner);
        (process, fut)
    }

    fn run(self, ctx: &mut T) {
        if let Some(p) = self.0.upgrade() {
            unsafe { p.run(ctx) }
        }
    }
}

#[repr(transparent)]
pub(crate) struct LocalProcessFuture<A: Asset>(Arc<Inner<A>>);

impl<A> Future for LocalProcessFuture<A>
where
    A: Asset,
{
    type Output = A::BuildFuture;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<A::BuildFuture> {
        let me = &*self.0;

        match me.state.compare_exchange(IDLE, POLLING, Acquire, Acquire) {
            Ok(IDLE) => {
                unsafe {
                    *me.waker.get() = Some(ctx.waker().clone());
                }

                match me.state.compare_exchange(POLLING, IDLE, AcqRel, Acquire) {
                    Err(IDLE) => {
                        unreachable!()
                    }
                    Ok(POLLING) => Poll::Pending,
                    Err(COMPLETE) => {
                        Poll::Ready(unsafe { (&mut *me.result.get()).take().unwrap() })
                    }
                    _ => unreachable!(),
                }
            }
            Err(POLLING) => {
                unreachable!()
            }
            Err(COMPLETE) => Poll::Ready(unsafe { (&mut *me.result.get()).take().unwrap() }),
            _ => unreachable!(),
        }
    }
}

struct LocalQueue<T> {
    processes: Vec<LocalProcess<T>>,
}

pub(crate) struct LocalProcessor {
    queues: RefCell<HashMap<TypeId, Box<dyn Any>>>,
}

impl LocalProcessor {
    pub fn new() -> Self {
        LocalProcessor {
            queues: RefCell::new(HashMap::new()),
        }
    }

    pub fn with_context<A>(&self, repr: A::Repr) -> LocalProcessFuture<A>
    where
        A: Asset,
    {
        let context_type_id = TypeId::of::<A::Context>();
        let (process, fut) = LocalProcess::new::<A>(repr);

        let mut lock = self.queues.borrow_mut();
        match lock.entry(context_type_id) {
            Entry::Occupied(mut entry) => {
                let queue = entry.get_mut();
                debug_assert!(<dyn Any>::is::<LocalQueue<A::Context>>(&**queue));
                let queue =
                    unsafe { &mut *(&mut **queue as *mut dyn Any as *mut LocalQueue<A::Context>) };
                queue.processes.push(process);
            }
            Entry::Vacant(entry) => {
                let queue = LocalQueue {
                    processes: vec![process],
                };
                entry.insert(Box::new(queue));
            }
        }
        drop(lock);

        fut
    }

    pub fn run<T: 'static>(&self, ctx: &mut T) {
        let context_type_id = TypeId::of::<T>();

        let mut lock = self.queues.borrow_mut();
        if let Some(queue) = lock.get_mut(&context_type_id) {
            debug_assert!(<dyn Any>::is::<LocalQueue<T>>(&**queue));

            let queue = unsafe { &mut *(&mut **queue as *mut dyn Any as *mut LocalQueue<T>) };
            let processes = std::mem::take(&mut queue.processes);
            drop(lock);

            for process in processes {
                process.run(ctx);
            }
        }
    }
}
