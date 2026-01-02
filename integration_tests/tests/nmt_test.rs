use zencan_client::nmt_master::NmtMaster;
use zencan_common::{nmt::NmtState, NodeId};
use zencan_node::{Callbacks, Node};

use integration_tests::prelude::*;

use serial_test::serial;

#[serial]
#[tokio::test]
async fn test_nmt_init() {
    const NODE_ID: u8 = 1;
    let od = &integration_tests::object_dict1::OD_TABLE;
    let state = &integration_tests::object_dict1::NODE_STATE;
    let mbox = &integration_tests::object_dict1::NODE_MBOX;

    let mut bus = SimBus::new();
    bus.add_node(mbox);
    let callbacks = Callbacks::new();
    let mut node = Node::new(NodeId::new(NODE_ID).unwrap(), callbacks, mbox, state, od);

    let _logger = BusLogger::new(bus.new_receiver());

    let sender = bus.new_sender();
    let receiver = bus.new_receiver();
    let mut master = NmtMaster::new(sender, receiver);

    assert_eq!(NmtState::Bootup, node.nmt_state());

    node.process(0);
    bus.flush_mailboxes();

    assert_eq!(NmtState::PreOperational, node.nmt_state());

    // Master should have received a boot up message
    let nodes = master.get_nodes();
    assert_eq!(1, nodes.len());
    assert_eq!(NODE_ID, nodes[0].id);
    assert_eq!(NmtState::PreOperational, nodes[0].state);

    // Broadcast start command
    master.nmt_start(0).await.unwrap();

    // Run a node process call
    node.process(0);
    bus.flush_mailboxes();

    assert_eq!(NmtState::Operational, node.nmt_state());
    assert_eq!(1, node.rx_message_count());

    master.nmt_stop(0).await.unwrap();
    // Run a node process call
    node.process(0);
    bus.flush_mailboxes();

    assert_eq!(NmtState::Stopped, node.nmt_state());
    assert_eq!(2, node.rx_message_count());
}
