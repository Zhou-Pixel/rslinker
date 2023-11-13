use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use std::collections::HashMap;
use std::hash::Hash;

pub struct Leader<S, R, I = i64>
where
    I: Clone,
{
    senders: HashMap<I, UnboundedSender<S>>,
    sender: UnboundedSender<(I, R)>,
    recver: UnboundedReceiver<(I, R)>,
}

impl<S, R, I> Default for Leader<S, R, I>
where
    I: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<S, R, I> Leader<S, R, I>
where
    I: Clone,
{
    pub async fn receive(&mut self) -> (I, R) {
        self.recver.recv().await.unwrap()
    }
}

impl<S, R, I> Leader<S, R, I>
where
    I: Eq + Hash + Clone,
{
    pub fn send_to(&self, id: &I, data: S) -> Result<(), SendError<S>> {
        match self.senders.get(id) {
            Some(sender) => sender.send(data),
            None => return Err(SendError(data)),
        }
    }
}

impl<S, R, I> Leader<S, R, I>
where
    I: Clone,
{
    pub fn new() -> Self {
        let (sender, recver) = mpsc::unbounded_channel();
        Self {
            sender,
            senders: Default::default(),
            recver,
        }
    }
}

impl<S, R, I> Leader<S, R, I>
where
    S: Clone,
    I: Clone,
{
    pub async fn broadcast(&self, data: &S) -> usize {
        let mut size = 0;
        for (_, sender) in &self.senders {
            if sender.send(data.clone()).is_ok() {
                size += 1;
            }
        }
        size
    }
}

impl<S, R, I> Leader<S, R, I>
where
    I: Eq + Hash + Clone,
{
    pub fn hire(&mut self, id: &I) -> Employee<S, R, I> {
        let (sender, recver) = mpsc::unbounded_channel::<S>();
        self.senders.insert(id.clone(), sender);
        Employee {
            // leader_senders: Arc::downgrade(&self.senders),
            id: id.clone(),
            sender: self.sender.clone(),
            recver,
        }
    }

    pub fn fired(&mut self, id: &I) -> UnboundedSender<S> {
        self.senders.remove(id).unwrap()
    }

    pub fn contains_employee(&self, id: &I) -> bool {
        self.senders.contains_key(id)
    }
}

pub struct Employee<R, S, I = i64>
where
    I: Clone,
{
    id: I,
    sender: UnboundedSender<(I, S)>,
    recver: UnboundedReceiver<R>,
}

impl<R, S, I> Employee<R, S, I>
where
    I: Clone,
{
    // pub async fn borrow(&self) -> watch::Ref<'_, R> {
    //     self.recver.borrow()
    // }

    // pub async fn borrow_and_update(&mut self) -> watch::Ref<'_, R> {
    //     self.recver.borrow_and_update()
    // }

    pub async fn wait(&mut self) -> Option<R> {
        self.recver.recv().await
    }

    pub fn report(&self, msg: S) -> Result<(), SendError<S>> {
        match self.sender.send((self.id.clone(), msg)) {
            Ok(()) => Ok(()),
            Err(err) => Err(SendError(err.0.1)),
        }
    }

    pub fn id(&self) -> &I {
        &self.id
    }
}

