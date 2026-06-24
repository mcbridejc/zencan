use std::time::Duration;

use assertables::assert_contains;
use zencan_client::LssMaster;
use zencan_common::protocol::{LssIdentity, NodeId};
use zencan_node::{Callbacks, Node};

use serial_test::serial;

use integration_tests::{object_dict1, object_dict2, prelude::*};

#[serial]
#[tokio::test]
async fn test_fast_scan() {
    let od1 = object_dict1::get_od();
    let od2 = object_dict2::get_od();
    let (mbox1, state1) = (od1.node_mbox(), od1.node_state());
    let (mbox2, state2) = (od2.node_mbox(), od2.node_state());
    // vendor/product/rev are set by device config
    // Manually set a serial number on each node

    assert_eq!(5000, od2.object1018().get_vendor());
    assert_eq!(0x1002, od2.object1018().get_product());
    assert_eq!(2, od2.object1018().get_revision());

    od1.object1018().set_serial(9999);
    od2.object1018().set_serial(5432);

    assert_eq!(9999, od1.object1018().get_serial());

    let mut bus = SimBus::new();
    bus.add_node(mbox1);
    bus.add_node(mbox2);
    let callbacks1 = Callbacks::new();
    let callbacks2 = Callbacks::new();
    let mut node1 = Node::new(NodeId::new(255).unwrap(), callbacks1, mbox1, state1, od1);
    let mut node2 = Node::new(NodeId::new(255).unwrap(), callbacks2, mbox2, state2, od2);

    let _logger = BusLogger::new(bus.new_receiver());

    const TIMEOUT: Duration = Duration::from_millis(25);
    let mut lss_master = LssMaster::new(bus.new_sender(), bus.new_receiver());

    test_with_background_process(
        &mut [&mut node1, &mut node2],
        &mut bus,
        move |mut ctx: TestContext| async move {
            let found_id = lss_master
                .fast_scan(TIMEOUT)
                .await
                .expect("No devices found by fastscan");
            let mut ids = vec![found_id];

            lss_master
                .set_node_id(100u8.try_into().unwrap())
                .await
                .expect("Failed setting node id");

            ctx.wait_for_process(2).await;

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
