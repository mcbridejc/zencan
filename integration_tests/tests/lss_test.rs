use std::time::Duration;

use assertables::assert_contains;
use zencan_client::LssMaster;
use zencan_common::{lss::LssIdentity, NodeId};
use zencan_node::{Callbacks, Node};

use serial_test::serial;

use integration_tests::{object_dict1, object_dict2, prelude::*};

#[serial]
#[tokio::test]
async fn test_fast_scan() {
    let (mbox1, state1, od1) = {
        (
            &object_dict1::NODE_MBOX,
            &object_dict1::NODE_STATE,
            &object_dict1::OD_TABLE,
        )
    };

    let (mbox2, state2, od2) = {
        (
            &object_dict2::NODE_MBOX,
            &object_dict2::NODE_STATE,
            &object_dict2::OD_TABLE,
        )
    };
    // vendor/product/rev are set by device config
    // Manually set a serial number on each node
    object_dict1::OBJECT1018.set_serial(9999);
    object_dict2::OBJECT1018.set_serial(5432);

    let mut bus = SimBus::new();
    bus.add_node(mbox1);
    bus.add_node(mbox2);
    let callbacks1 = Callbacks::new();
    let callbacks2 = Callbacks::new();
    let mut node1 = Node::new(NodeId::new(255).unwrap(), callbacks1, mbox1, state1, od1);
    let mut node2 = Node::new(NodeId::new(255).unwrap(), callbacks2, mbox2, state2, od2);

    let _logger = BusLogger::new(bus.new_receiver());

    const TIMEOUT: Duration = Duration::from_millis(5);
    let mut lss_master = LssMaster::new(bus.new_sender(), bus.new_receiver());

    test_with_background_process(
        &mut [&mut node1, &mut node2],
        &mut bus,
        move |_ctx| async move {
            let found_id = lss_master
                .fast_scan(TIMEOUT)
                .await
                .expect("No devices found by fastscan");
            let mut ids = vec![found_id];

            lss_master
                .set_node_id(100u8.try_into().unwrap())
                .await
                .expect("Failed setting node id");

            let found_id = lss_master
                .fast_scan(TIMEOUT)
                .await
                .expect("No devices found by second fastscan");
            ids.push(found_id);
            lss_master
                .set_node_id(101u8.try_into().unwrap())
                .await
                .expect("Failed setting second node id");

            let exp1 = LssIdentity {
                vendor_id: 1234,
                product_code: 12000,
                revision: 1,
                serial: 9999,
            };

            let exp2 = LssIdentity {
                vendor_id: 5000,
                product_code: 0x1002,
                revision: 2,
                serial: 5432,
            };

            println!("Found IDs: {ids:?}");
            assert_contains!(ids, &exp1);
            assert_contains!(ids, &exp2);
        },
    )
    .await;
}
