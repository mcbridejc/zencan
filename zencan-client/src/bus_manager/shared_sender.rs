//! Utility for sharing a single socket among tasks
use std::sync::Arc;
use tokio::sync::Mutex;

use zencan_common::{traits::AsyncCanSender, CanMessage};

#[derive(Debug)]
pub struct SharedSender<S: AsyncCanSender> {
    inner: Arc<Mutex<S>>,
}

impl<S: AsyncCanSender> Clone for SharedSender<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S: AsyncCanSender> SharedSender<S> {
    pub fn new(sender: Arc<Mutex<S>>) -> Self {
        Self { inner: sender }
    }

    async fn send(&mut self, msg: CanMessage) -> Result<(), S::Error> {
        let mut inner = self.inner.lock().await;
        inner.send(msg).await
    }
}

impl<S: AsyncCanSender> AsyncCanSender for SharedSender<S> {
    type Error = S::Error;
    fn send(
        &mut self,
        msg: CanMessage,
    ) -> impl core::future::Future<Output = Result<(), Self::Error>> {
        self.send(msg)
    }
}
