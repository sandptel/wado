//! A tiny blocking executor for the event thread: `Core::next_event` only awaits a `Notify`,
//! so parking the thread until woken is all it needs.
use std::future::Future;

use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;

pub struct Executor;

struct Unpark(Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

impl Executor {
    pub fn block_on<F: Future>(&self, f: F) -> F::Output {
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut f = Box::pin(f);
        loop {
            if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
            std::thread::park();
        }
    }
}
