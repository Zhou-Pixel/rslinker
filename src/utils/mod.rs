pub mod chat;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Never;

impl Never {
    pub fn never() -> Self {
        Never {}
    }
}

impl Future for Never {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
