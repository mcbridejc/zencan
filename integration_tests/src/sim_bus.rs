use std::sync::{Arc, Mutex};

use zencan_common::messages::CanMessage;
use zencan_common::traits::{AsyncCanReceiver, AsyncCanSender};
use zencan_node::NodeMbox;

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Clone, Default)]
pub struct SimBus<'a> {
    mailboxes: Arc<Mutex<Vec<&'a NodeMbox>>>,
    // None node external channels for sending messages to, e.g. test listeners
    external_channels: Arc<Mutex<Vec<UnboundedSender<CanMessage>>>>,
}

impl<'a> SimBus<'a> {
    pub fn new() -> Self {
        Self {
            mailboxes: Arc::new(Mutex::new(Vec::new())),
            external_channels: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn add_node(
        &mut self,
        mbox: &'a NodeMbox,
    ) -> impl Fn(CanMessage) -> Result<(), CanMessage> + 'a {
        let mut mailboxes = self.mailboxes.lock().unwrap();
        let node_index = mailboxes.len();
        mailboxes.push(mbox);
        let mailboxes = self.mailboxes.clone();
        let external_channels = self.external_channels.clone();
        move |msg: CanMessage| {
            for (i, mbox) in mailboxes.lock().unwrap().iter().enumerate() {
                // Deliver to all node mailboxes, except for the sender
                if i != node_index {
                    mbox.store_message(msg).ok();
                }
            }
            for ext in external_channels.lock().unwrap().iter() {
                ext.send(msg).unwrap()
            }
            Ok(())
        }
    }

    pub fn new_receiver(&mut self) -> SimBusReceiver {
        let (tx, rx) = unbounded_channel();
        self.external_channels.lock().unwrap().push(tx);
        SimBusReceiver { channel_rx: rx }
    }

    pub fn new_sender(&mut self) -> SimBusSender<'a> {
        SimBusSender {
            node_states: self.mailboxes.clone(),
            external_channels: self.external_channels.clone(),
        }
    }
}

pub struct SimBusSender<'a> {
    node_states: Arc<Mutex<Vec<&'a NodeMbox>>>,
    external_channels: Arc<Mutex<Vec<UnboundedSender<CanMessage>>>>,
}

impl AsyncCanSender for SimBusSender<'_> {
    async fn send(&mut self, msg: CanMessage) -> Result<(), CanMessage> {
        // Send to nodes on the bus
        for ns in self.node_states.lock().unwrap().iter() {
            // It doesn't matter if store message fails; that just means the node did not
            // recognize/accept the message
            ns.store_message(msg).ok();
        }
        // Send to external listeners on the bus (those created by `new_receiver()``)
        for rx in self.external_channels.lock().unwrap().iter() {
            rx.send(msg).unwrap();
        }

        Ok(())
    }
}

pub struct SimBusReceiver {
    channel_rx: UnboundedReceiver<CanMessage>,
}

impl SimBusReceiver {
    pub fn flush(&mut self) {
        while self.channel_rx.try_recv().is_ok() {}
    }
}

impl AsyncCanReceiver for SimBusReceiver {
    type Error = ();

    async fn recv(&mut self) -> Result<CanMessage, Self::Error> {
        self.channel_rx.recv().await.ok_or(())
    }

    fn try_recv(&mut self) -> Option<CanMessage> {
        self.channel_rx.try_recv().ok()
    }

    fn flush(&mut self) {
        while self.channel_rx.try_recv().is_ok() {}
    }
}
