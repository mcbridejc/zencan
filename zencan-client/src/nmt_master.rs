//! Simple interface for sending NMT commands to a bus
use std::time::Instant;

use zencan_common::{
    can::{AsyncCanReceiver, AsyncCanSender, CanMessage},
    protocol::{NmtCommand, NmtCommandSpecifier, NmtState, ZencanMessage},
};

type Result<T> = std::result::Result<T, ()>;

/// Represents the information about a single node detected on the bus by the [NmtMaster]
#[derive(Copy, Clone, Debug)]
pub struct Node {
    /// The ID of the node
    pub id: u8,
    /// The last NMT state reported by the node
    pub state: NmtState,
    /// The time when the last heartbeat message from received from the node
    pub last_status: Instant,
    last_toggle: bool,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            id: 0,
            state: NmtState::Bootup,
            last_status: Instant::now(),
            last_toggle: true,
        }
    }
}

const MAX_NODES: usize = 127;

#[derive(Debug)]
/// An NMT master which allows monitoring the bus for heartbeats and commanding state changes
pub struct NmtMaster<S, R> {
    sender: S,
    receiver: R,
    nodes: [Node; MAX_NODES],
}

impl<S: AsyncCanSender, R: AsyncCanReceiver> NmtMaster<S, R> {
    /// Create a new NmtMaster
    ///
    /// # Arguments
    /// - `sender`: An object which implements [`AsyncCanSender`] to be used for sending messages to
    ///   the bus
    /// - `receiver`: An object which implements [`AsyncCanReceiver`] to be used for receiving
    ///   messages from the bus
    ///
    /// When using socketcan, these can be created with [`crate::open_socketcan`].
    pub fn new(sender: S, receiver: R) -> Self {
        let nodes = [Node::default(); MAX_NODES];
        Self {
            sender,
            receiver,
            nodes,
        }
    }

    /// Receive and process all messages available from the message receiver
    pub fn process_rx(&mut self) {
        while let Some(msg) = self.receiver.try_recv() {
            self.handle_message(msg);
        }
    }

    fn handle_message(&mut self, msg: CanMessage) {
        // Attempt to convert the raw message into a zencanMessage. This may fail, e.g. if
        // non zencan messages are received, and that's OK; those are ignored.
        let open_msg: ZencanMessage = match msg.try_into() {
            Ok(m) => m,
            Err(_) => return,
        };

        if let ZencanMessage::Heartbeat(heartbeat) = open_msg {
            self.handle_heartbeat(heartbeat.node, heartbeat.state, heartbeat.toggle)
        }
    }

    /// Get a list of all nodes detected on the bus via heartbeat/reset messages
    pub fn get_nodes(&mut self) -> &[Node] {
        self.process_rx();

        // Find the first empty slot; this indicates the end of the list
        let n = self
            .nodes
            .iter()
            .position(|n| n.id == 0)
            .unwrap_or(MAX_NODES);
        &self.nodes[0..n]
    }

    fn handle_heartbeat(&mut self, node: u8, state: NmtState, toggle: bool) {
        // Find the node in the ordered list, inserting if needed.
        for i in 0..self.nodes.len() {
            let list_node = &mut self.nodes[i];
            if list_node.id == node {
                // Node already in list. Update it
                list_node.last_status = Instant::now();
                list_node.last_toggle = toggle;
                list_node.state = state;
                break;
            } else if list_node.id == 0 || list_node.id > node {
                // Found end of list or higher node - insert here
                // Shift all higher nodes
                for j in (i + 1..self.nodes.len()).rev() {
                    self.nodes[j] = self.nodes[j - 1];
                }
                self.nodes[i] = Node {
                    id: node,
                    state,
                    last_status: Instant::now(),
                    last_toggle: toggle,
                };
                break;
            }
        }
    }

    /// Send application reset command
    ///
    /// # Arguments
    ///
    /// - `node`: The node ID to command, or 0 to broadcast to all nodes
    pub async fn nmt_reset_app(&mut self, node: u8) -> Result<()> {
        self.send_nmt_cmd(NmtCommandSpecifier::ResetApp, node).await
    }

    /// Send communications reset command
    ///
    /// # Arguments
    ///
    /// - `node`: The node ID to command, or 0 to broadcast to all nodes
    pub async fn nmt_reset_comms(&mut self, node: u8) -> Result<()> {
        self.send_nmt_cmd(NmtCommandSpecifier::ResetComm, node)
            .await
    }

    /// Send start operation command
    ///
    /// # Arguments
    ///
    /// - `node`: The node ID to command, or 0 to broadcast to all nodes
    pub async fn nmt_start(&mut self, node: u8) -> Result<()> {
        self.send_nmt_cmd(NmtCommandSpecifier::Start, node).await
    }

    /// Send stop operation command
    ///
    /// # Arguments
    ///
    /// - `node`: The node ID to command, or 0 to broadcast to all nodes
    pub async fn nmt_stop(&mut self, node: u8) -> Result<()> {
        self.send_nmt_cmd(NmtCommandSpecifier::Stop, node).await
    }

    /// Send enter pre-operational command
    ///
    /// # Arguments
    ///
    /// - `node`: The node ID to command, or 0 to broadcast to all nodes
    pub async fn nmt_enter_preop(&mut self, node: u8) -> Result<()> {
        self.send_nmt_cmd(NmtCommandSpecifier::EnterPreOp, node)
            .await
    }

    async fn send_nmt_cmd(&mut self, cmd: NmtCommandSpecifier, node: u8) -> Result<()> {
        let message = NmtCommand { cs: cmd, node };
        self.sender.send(message.into()).await.map_err(|_| ())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::{channel, Receiver, Sender};
    use zencan_common::{can::CanSendError, protocol::Heartbeat, CanId};

    use super::*;

    #[derive(Debug)]
    struct MockSendError(CanMessage);

    impl CanSendError for MockSendError {
        fn into_can_message(self) -> CanMessage {
            self.0
        }

        fn message(&self) -> String {
            "mock send failed".into()
        }
    }

    struct MockSender {
        tx: Sender<CanMessage>,
    }

    impl AsyncCanSender for MockSender {
        type Error = MockSendError;

        async fn send(&mut self, msg: CanMessage) -> std::result::Result<(), Self::Error> {
            self.tx.try_send(msg).map_err(|_| MockSendError(msg))
        }
    }

    struct MockReceiver {
        rx: Receiver<CanMessage>,
    }

    #[derive(Debug)]
    struct MockReceiveError;

    impl AsyncCanReceiver for MockReceiver {
        type Error = MockReceiveError;

        fn try_recv(&mut self) -> Option<CanMessage> {
            self.rx.try_recv().ok()
        }

        async fn recv(&mut self) -> std::result::Result<CanMessage, Self::Error> {
            self.rx.recv().await.ok_or(MockReceiveError)
        }
    }

    /// Returns the master, the bus side of its receiver, and the bus side of its sender
    fn mock_master() -> (
        NmtMaster<MockSender, MockReceiver>,
        Sender<CanMessage>,
        Receiver<CanMessage>,
    ) {
        let (to_master, master_rx) = channel(16);
        let (master_tx, from_master) = channel(16);
        let master = NmtMaster::new(MockSender { tx: master_tx }, MockReceiver { rx: master_rx });
        (master, to_master, from_master)
    }

    fn heartbeat(node: u8, state: NmtState) -> CanMessage {
        Heartbeat {
            node,
            toggle: false,
            state,
        }
        .into()
    }

    #[tokio::test]
    async fn keeps_nodes_heard_out_of_order() {
        let (mut master, bus, _) = mock_master();

        bus.send(heartbeat(3, NmtState::Operational)).await.unwrap();
        bus.send(heartbeat(1, NmtState::PreOperational))
            .await
            .unwrap();
        bus.send(heartbeat(2, NmtState::Stopped)).await.unwrap();

        let nodes: Vec<(u8, NmtState)> =
            master.get_nodes().iter().map(|n| (n.id, n.state)).collect();
        assert_eq!(
            nodes,
            vec![
                (1, NmtState::PreOperational),
                (2, NmtState::Stopped),
                (3, NmtState::Operational),
            ]
        );
    }

    #[tokio::test]
    async fn enter_preop_sends_command() {
        let (mut master, _, mut bus) = mock_master();

        master.nmt_enter_preop(5).await.unwrap();

        let msg = bus.try_recv().unwrap();
        assert_eq!(msg.id, CanId::std(0));
        assert_eq!(msg.data(), &[0x80, 5]);
    }
}
