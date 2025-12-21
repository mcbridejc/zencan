use std::sync::{Arc, Mutex};

use zencan_common::messages::CanMessage;
use zencan_common::traits::{AsyncCanReceiver, AsyncCanSender, CanSendError};
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

    pub fn flush_mailboxes(&self) {
        let mailboxes = self.mailboxes.lock().unwrap();
        let external_channels = self.external_channels.lock().unwrap();

        for (i, sending_mbox) in mailboxes.iter().enumerate() {
            while let Some(sent_frame) = sending_mbox.next_transmit_message() {
                for (j, receiving_mbox) in mailboxes.iter().enumerate() {
                    if i == j {
                        // Don't send the message back to the node that sent it
                        continue;
                    }
                    receiving_mbox.store_message(sent_frame).ok();
                }

                // Send to all non-node listeners
                for ext in external_channels.iter() {
                    ext.send(sent_frame).unwrap()
                }
            }
        }
    }

    pub fn add_node(&mut self, mbox: &'a NodeMbox) {
        let mut mailboxes = self.mailboxes.lock().unwrap();
        mailboxes.push(mbox);
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

/// Create an error type for sim bus sender.
///
/// The sender can't fail, so this type is never instantiated
#[derive(Debug)]
pub struct SimBusSendError(());

impl CanSendError for SimBusSendError {
    fn into_can_message(self) -> CanMessage {
        panic!("uninstantiable")
    }

    fn message(&self) -> String {
        String::new()
    }
}

impl AsyncCanSender for SimBusSender<'_> {
    type Error = SimBusSendError;
    async fn send(&mut self, msg: CanMessage) -> Result<(), SimBusSendError> {
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
