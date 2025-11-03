pub mod executor;
pub mod keyboard;


use alloc::boxed::Box;
use core::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
    fmt,
};



pub struct Task {
    id: TaskId,
    future: Pin<Box<dyn Future<Output = ()> + Send>>, // ← add Send here
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + Send + 'static) -> Task { // ← add Send
        Task {
            id: TaskId::new(),
            future: Box::pin(future),
        }
    }

    pub fn poll(&mut self, context: &mut Context<'_>) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(u64);

impl TaskId {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

// implement Display so `format!("{}", id)` works
impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // print numeric id; change format if you want hex or with prefix
        write!(f, "{}", self.0)
    }
}