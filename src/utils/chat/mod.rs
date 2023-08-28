
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use std::collections::HashMap;
use std::hash::Hash;

// use tokio::sync::RwLock;
// use tokio::sync::RwLockReadGuard;
// use tokio::sync::RwLockWriteGuard;

// pub fn new<LS, LR, I>(size: Option<usize>, id: I) -> (Leader<LS, LR, I>, Employee<LS, LR, I>)
// where
//     I: Clone
// {
//     let (watch_tx, watch_rx) = mpsc::channel(size.unwrap_or(1000));
//     let (mpsc_tx, mpsc_rx) = mpsc::channel(size.unwrap_or(1000));

//     (
//         Leader {
//             sender: watch_tx,
//             recver: mpsc_rx,
//         },
//         Employee {
//             sender: mpsc_tx,
//             recver: watch_rx,
//         },
//     )
// }


// pub struct Leader<S, R, I = i64> {
//     inner: Arc<RwLock<private::Leader<S, R, I>>>,
//     // size: usize,
//     // senders: HashMap<I, Sender<S>>,
//     // sender: Sender<R>,
//     // recver: Receiver<R>,
// }

// impl<S, R, I> Default for Leader<S, R, I> {
//     fn default() -> Self {
//         Self::new(None)
//     }
// }

// impl<S, R, I> Leader<S, R, I> {
//     pub async fn receive(&self) -> R {
//         // self.recver.recv().await.unwrap()
//         self.get_inner_mut().await.receive().await
//     }

//     async fn get_inner(&self) -> RwLockReadGuard<private::Leader<S, R, I>> {
//         self.inner.read().await
//     }
//     async fn get_inner_mut(&self) -> RwLockWriteGuard<private::Leader<S, R, I>> {
//         self.inner.write().await
//     }
// }

// impl<S, R, I> Leader<S, R, I>
// where
//     I: Eq + Hash,
// {
//     pub async fn send_to(&self, id: &I, data: S) -> Result<(), SendError<S>> {
//         self.get_inner().await.send_to(id, data).await
//         // self.senders.get(id).as_ref().unwrap().send(data).await
//     }
// }

// // impl<S, R, I> Leader<S, R, I>
// // where
// //     I: Eq + Hash + Clone,
// //     S: Clone,
// // {
// //     pub async fn new(&self, data: &S) -> Result<(), Vec<&I>> {

// //     }
// // }

// impl<S, R, I> Leader<S, R, I> {
//     pub fn new(size: Option<usize>) -> Self {
//         // let size = size.unwrap_or(1000);
//         // let (sender, recver) = mpsc::channel(size);
//         Self {
//             inner: Arc::new(RwLock::new(private::Leader::new(size)))
//             // size,
//             // sender,
//             // senders: Default::default(),
//             // recver,
//         }
//     }
// }

// impl<S, R, I> Leader<S, R, I>
// where
//     S: Clone,
//     I: Clone
// {
//     pub async fn broadcast(&self, data: &S) -> Result<(), Vec<I>> {
//         // todo!()
//         let inner = self.get_inner().await;
//         let Err(fail) = inner.broadcast(data).await else {
//             return Ok(());
//         };
//         Err(fail.into_iter().map(|v| v.clone()).collect::<Vec<I>>())
//         // let mut fail = Vec::new();
//         // for (id, ref sender) in &self.senders {
//         //     if sender.send(data.clone()).await.is_err() {
//         //         fail.push(id);
//         //     }
//         // }
//         // if fail.is_empty() {
//         //     Ok(())
//         // } else {
//         //     Err(fail)
//         // }
//     }
// }

// impl<S, R, I> Leader<S, R, I>
// where
//     I: Eq + Hash + Clone,
// {
//     pub async fn hire(&mut self, id: &I) -> Employee<S, R, I> {
//         let inner = Arc::new(RwLock::new(self.get_inner_mut().await.hire(id)));
//         let leader = Arc::downgrade(&self.inner);
//         Employee {
//             inner,
//             leader
//         }
//         // let (sender, recver) = mpsc::channel::<S>(self.size);
//         // self.senders.insert(id.clone(), sender);
//         // Employee {
//         //     id: id.clone(),
//         //     sender: self.sender.clone(),
//         //     recver,
//         // }
//     }

//     pub async fn fired(&mut self, id: &I) {
//         self.get_inner_mut().await.fired(id);
//         // self.senders.remove(id);
//     }
// }

// pub struct Employee<R, S, I = i64> {
//     inner: Arc<RwLock<private::Employee<R, S, I>>>,
//     leader: Weak<RwLock<private::Leader<R, S, I>>>
//     // id: I,
//     // sender: Sender<S>,
//     // recver: Receiver<R>,
// }


// impl<R, S, I> Employee<R, S, I> {
//     // pub async fn borrow(&self) -> watch::Ref<'_, R> {
//     //     self.recver.borrow()
//     // }

//     // pub async fn borrow_and_update(&mut self) -> watch::Ref<'_, R> {
//     //     self.recver.borrow_and_update()
//     // }

//     async fn get_inner(&self) -> RwLockReadGuard<'_, private::Employee<R, S, I>> {
//         self.inner.read().await
//     }

//     async fn get_inner_mut(&self) -> RwLockWriteGuard<'_, private::Employee<R, S, I>> {
//         self.inner.write().await
//     }

//     pub async fn wait(&self) -> Option<R> {
//         self.get_inner_mut().await.wait().await
//         // self.recver.recv().await
//     }

//     pub async fn report(&self, msg: S) -> Result<(), SendError<S>> {
//         self.get_inner().await.report(msg).await
//         // self.sender.send(msg).await
//     }

//     // pub async fn id<'a>(&'a self) -> &'a I {
//     //     let id = self.get_inner().await.id();
//     //     id
//     // }
// }


// impl<R, S, I: Clone> Employee<R, S, I> { 
//     pub async fn id(&self) -> I {
//         let inner = self.get_inner().await;
//         let id = inner.id();
//         id.clone()
//     }
// }

// mod private {
    // use tokio::sync::mpsc::error::SendError;
    // use tokio::sync::mpsc::{self, Receiver, Sender};
    
    // use std::collections::HashMap;
    // use std::hash::Hash; 

    pub struct Leader<S, R, I = i64> 
    where
        I: Clone
    {
        senders: HashMap<I, UnboundedSender<S>>,
        sender: UnboundedSender<(I, R)>,
        recver: UnboundedReceiver<(I, R)>,
    }
    
    impl<S, R, I> Default for Leader<S, R, I> 
    where
        I: Clone
    {
        fn default() -> Self {
            Self::new()
        }
    }
    
    impl<S, R, I> Leader<S, R, I> 
    where
        I: Clone
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
            self.senders.get(id).as_ref().unwrap().send(data)
        }
    }
    
    // impl<S, R, I> Leader<S, R, I>
    // where
    //     I: Eq + Hash + Clone,
    //     S: Clone,
    // {
    //     pub async fn new(&self, data: &S) -> Result<(), Vec<&I>> {
    
    //     }
    // }
    
    impl<S, R, I> Leader<S, R, I> 
    where
        I: Clone
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
        I: Clone
    {
        pub async fn broadcast(&self, data: &S) -> usize {
            let mut size = 0;
            for (_, ref sender) in &self.senders {
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
        I: Clone
    {
        id: I,
        sender: UnboundedSender<(I, S)>,
        recver: UnboundedReceiver<R>
    }
    
    
    impl<R, S, I> Employee<R, S, I>
    where
        I: Clone
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
    
        pub fn report(&self, msg: S) -> Result<(), SendError<(I, S)>> {
            self.sender.send((self.id.clone(), msg))
        }
    
        pub fn id(&self) -> &I {
            &self.id
        }

    }

    // impl<R, S, I> Employee<R, S, I> 
    // where 
    //     I: Eq + Hash + Clone,
    // {
    //     pub async fn become_regular(&mut self) -> bool {
    //         let Some(senders) = self.leader_senders.upgrade() else {
    //             return false;
    //         };
    //         let (sender, recver) = mpsc::unbounded_channel::<R>();
    //         senders.write().await.insert(self.id.clone(), sender);
    //         self.recver = Some(recver);
    //         true
    //     }
    //     pub async fn resign(&self) {
    //         let Some(senders) = self.leader_senders.upgrade() else {
    //             return;
    //         };
    //         senders.write().await.remove(&self.id);
    //     }
    // }
// }
