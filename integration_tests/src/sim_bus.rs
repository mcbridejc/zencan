use std::sync::{Arc, Mutex};

use zencan_common::can::{
    AsyncCanReceiver, AsyncCanSender, CanFrame, CanMessage, CanSendError, ClassicAdapter,
};
use zencan_node::{NodeFrame, NodeMbox};

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// A node attached to a [`SimBus`] carrying frames of type `F`
#[derive(Clone, Copy)]
enum SimNode<'a, F: NodeFrame> {
    /// A node exchanging the bus frame type
    Native(&'a NodeMbox<F>),
    /// A classic CAN node on the bus, as with a CAN FD tolerant controller: it only sees the frames
    /// which fit in 8 bytes, and its frames are carried in the bus frame type
    Classic(&'a NodeMbox),
}

impl<F: NodeFrame> SimNode<'_, F> {
    fn store_message(&self, frame: F) {
        // It doesn't matter if store message fails; that just means the node did not
        // recognize/accept the message
        match self {
            SimNode::Native(mbox) => {
                mbox.store_message(frame).ok();
            }
            SimNode::Classic(mbox) => {
                if let Ok(msg) = <CanMessage as CanFrame>::try_new(frame.id(), frame.data()) {
                    mbox.store_message(msg).ok();
                }
            }
        }
    }

    fn next_transmit_message(&self) -> Option<F> {
        match self {
            SimNode::Native(mbox) => mbox.next_transmit_message(),
            SimNode::Classic(mbox) => {
                while let Some(msg) = mbox.next_transmit_message() {
                    if let Ok(frame) = F::try_from(msg) {
                        return Some(frame);
                    }
                }
                None
            }
        }
    }
}

/// A simulated bus, carrying classic CAN frames by default, or CAN FD frames for
/// `SimBus<CanFdMessage>`
#[derive(Clone, Default)]
pub struct SimBus<'a, F: NodeFrame = CanMessage> {
    mailboxes: Arc<Mutex<Vec<SimNode<'a, F>>>>,
    // None node external channels for sending messages to, e.g. test listeners
    external_channels: Arc<Mutex<Vec<UnboundedSender<F>>>>,
}

impl<'a, F: NodeFrame> SimBus<'a, F> {
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
                    receiving_mbox.store_message(sent_frame);
                }

                // Send to all non-node listeners
                for ext in external_channels.iter() {
                    ext.send(sent_frame).unwrap()
                }
            }
        }
    }

    pub fn add_node(&mut self, mbox: &'a NodeMbox<F>) {
        let mut mailboxes = self.mailboxes.lock().unwrap();
        mailboxes.push(SimNode::Native(mbox));
    }

    /// Attach a classic CAN node, e.g. to a CAN FD bus
    pub fn add_classic_node(&mut self, mbox: &'a NodeMbox) {
        let mut mailboxes = self.mailboxes.lock().unwrap();
        mailboxes.push(SimNode::Classic(mbox));
    }

    pub fn new_receiver(&mut self) -> SimBusReceiver<F> {
        let (tx, rx) = unbounded_channel();
        self.external_channels.lock().unwrap().push(tx);
        SimBusReceiver { channel_rx: rx }
    }

    pub fn new_sender(&mut self) -> SimBusSender<'a, F> {
        SimBusSender {
            node_states: self.mailboxes.clone(),
            external_channels: self.external_channels.clone(),
        }
    }

    /// Create a sender for classic CAN frames, which are carried in the bus frame type
    pub fn new_classic_sender(&mut self) -> ClassicAdapter<SimBusSender<'a, F>> {
        ClassicAdapter::new(self.new_sender())
    }

    /// Create a receiver for the frames of the bus which fit in a classic CAN frame
    pub fn new_classic_receiver(&mut self) -> ClassicAdapter<SimBusReceiver<F>> {
        ClassicAdapter::new(self.new_receiver())
    }
}

pub struct SimBusSender<'a, F: NodeFrame = CanMessage> {
    node_states: Arc<Mutex<Vec<SimNode<'a, F>>>>,
    external_channels: Arc<Mutex<Vec<UnboundedSender<F>>>>,
}

/// Create an error type for sim bus sender.
///
/// The sender can't fail, so this type is never instantiated
#[derive(Debug)]
pub struct SimBusSendError(());

impl<F: CanFrame> CanSendError<F> for SimBusSendError {
    fn into_can_message(self) -> F {
        panic!("uninstantiable")
    }

    fn message(&self) -> String {
        String::new()
    }
}

impl<F: NodeFrame> AsyncCanSender<F> for SimBusSender<'_, F> {
    type Error = SimBusSendError;
    async fn send(&mut self, msg: F) -> Result<(), SimBusSendError> {
        // Send to nodes on the bus
        for ns in self.node_states.lock().unwrap().iter() {
            ns.store_message(msg);
        }
        // Send to external listeners on the bus (those created by `new_receiver()``)
        for rx in self.external_channels.lock().unwrap().iter() {
            rx.send(msg).unwrap();
        }

        Ok(())
    }
}

pub struct SimBusReceiver<F: CanFrame = CanMessage> {
    channel_rx: UnboundedReceiver<F>,
}

impl<F: CanFrame> SimBusReceiver<F> {
    pub fn flush(&mut self) {
        while self.channel_rx.try_recv().is_ok() {}
    }
}

impl<F: CanFrame> AsyncCanReceiver<F> for SimBusReceiver<F> {
    type Error = ();

    async fn recv(&mut self) -> Result<F, Self::Error> {
        self.channel_rx.recv().await.ok_or(())
    }

    fn try_recv(&mut self) -> Option<F> {
        self.channel_rx.try_recv().ok()
    }

    fn flush(&mut self) {
        while self.channel_rx.try_recv().is_ok() {}
    }
}
