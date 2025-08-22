//! Utility for sharing a single socket among tasks
//!

use std::sync::Arc;
use std::sync::Mutex;

use tokio::sync::mpsc::error::TrySendError;
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use zencan_common::{traits::AsyncCanReceiver, CanMessage};

#[derive(Clone, Copy, Debug)]
pub struct NoMsgError;

#[derive(Debug)]
struct SharedRecieiverInner {
    senders: Vec<Sender<CanMessage>>,
}

impl SharedRecieiverInner {
    pub fn create_rx(&mut self) -> Receiver<CanMessage> {
        let (tx, rx) = channel(100);
        self.senders.push(tx);
        rx
    }
}

#[derive(Debug)]
pub struct SharedReceiver {
    _task_handle: JoinHandle<()>,
    inner: Arc<Mutex<SharedRecieiverInner>>,
}

impl SharedReceiver {
    pub fn new<R: AsyncCanReceiver + Send + 'static>(mut receiver: R) -> Self {
        let inner = Arc::new(Mutex::new(SharedRecieiverInner {
            senders: Vec::new(),
        }));
        let inner_clone = inner.clone();
        let task_handle = tokio::spawn(async move {
            loop {
                if let Ok(msg) = receiver.recv().await {
                    let mut inner = inner_clone.lock().unwrap();
                    inner.senders.retain(|sender| {
                        if let Err(e) = sender.try_send(msg) {
                            return match e {
                                TrySendError::Full(_) => {
                                    log::warn!("Dropped received message due to overflow");
                                    true
                                }
                                TrySendError::Closed(_) => false,
                            };
                        }

                        true
                    });
                };
            }
        });
        Self {
            _task_handle: task_handle,
            inner,
        }
    }

    pub fn create_rx(&mut self) -> SharedReceiverChannel {
        let rx = self.inner.lock().unwrap().create_rx();

        SharedReceiverChannel {
            inner: self.inner.clone(),
            receiver: rx,
        }
    }

    /// Get the number of current receiver channels
    #[allow(dead_code)]
    pub fn num_channels(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.senders.len()
    }
}

#[derive(Debug)]
pub struct SharedReceiverChannel {
    /// Data shared with the multi consumer Rx
    inner: Arc<Mutex<SharedRecieiverInner>>,
    /// Our receive channel
    receiver: Receiver<CanMessage>,
}

impl Clone for SharedReceiverChannel {
    fn clone(&self) -> Self {
        let receiver = self.inner.lock().unwrap().create_rx();
        Self {
            inner: self.inner.clone(),
            receiver,
        }
    }
}

#[allow(dead_code)]
impl SharedReceiverChannel {
    /// Remove any pending messages from the queue
    pub fn flush(&mut self) {
        // Clear our queue
        while let Ok(_msg) = self.receiver.try_recv() {}
    }

    pub async fn recv(&mut self) -> Result<CanMessage, NoMsgError> {
        self.receiver.recv().await.ok_or(NoMsgError)
    }

    pub fn try_recv(&mut self) -> Option<CanMessage> {
        self.receiver.try_recv().ok()
    }
}

impl AsyncCanReceiver for SharedReceiverChannel {
    type Error = NoMsgError;

    fn try_recv(&mut self) -> Option<CanMessage> {
        self.try_recv()
    }

    fn recv(&mut self) -> impl core::future::Future<Output = Result<CanMessage, Self::Error>> {
        self.recv()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use zencan_common::CanId;

    use super::*;

    struct MockReceiver {
        rx: Receiver<CanMessage>,
    }

    impl MockReceiver {
        pub fn new(rx: Receiver<CanMessage>) -> Self {
            Self { rx }
        }
    }

    #[derive(Debug)]
    struct MockReceiveError {}

    impl AsyncCanReceiver for MockReceiver {
        type Error = MockReceiveError;

        fn try_recv(&mut self) -> Option<CanMessage> {
            let result = self.rx.try_recv();
            println!("{result:?}");
            result.ok()
        }

        async fn recv(&mut self) -> Result<CanMessage, Self::Error> {
            self.rx.recv().await.ok_or(MockReceiveError {})
        }
    }

    #[tokio::test]
    async fn test_shared_receiver() {
        let (chan_tx, chan_rx) = channel(8);
        let can_receiver = MockReceiver::new(chan_rx);
        let mut shared_receiver = SharedReceiver::new(can_receiver);

        let mut channel_a = shared_receiver.create_rx();
        let mut channel_b = shared_receiver.create_rx();

        let msg100 = CanMessage::new(CanId::std(100), &[0, 1, 2, 3]);
        chan_tx.send(msg100.clone()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(Some(msg100), channel_a.try_recv());
        assert_eq!(Some(msg100), channel_b.try_recv());

        assert_eq!(None, channel_a.try_recv());
        assert_eq!(None, channel_a.try_recv());
        // Drop a channel, and make sure the num channels goes down after message is processed
        drop(channel_a);

        chan_tx.send(msg100.clone()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(msg100, channel_b.recv().await.unwrap());

        assert_eq!(1, shared_receiver.num_channels());
    }
}
