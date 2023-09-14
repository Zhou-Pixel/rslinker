pub mod chat;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::sync::Arc;
use fast_async_mutex::RwLock;


pub type ARwLock<T> = Arc<RwLock<T>>;


pub struct Never;

impl Never {
    #[inline]
    pub const fn never() -> Self {
        Never {}
    }
}

impl Future for Never {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
