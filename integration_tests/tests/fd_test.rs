//! End to end tests of a CAN FD node
//!
//! The node is generated from `device_configs/example4_fd.toml`, which selects
//! `frame_format = "fd"`, and runs on a simulated CAN FD bus.
use integration_tests::{object_dict1, object_dict4, prelude::*};
use serial_test::serial;
use zencan_client::nmt_master::NmtMaster;
use zencan_common::can::{AsyncCanReceiver, AsyncCanSender, CanFdMessage, CanFrame, CanId};
use zencan_node::object_dict::ObjectAccess as _;

const FD_NODE_ID: u8 = 3;
const CLASSIC_NODE_ID: u8 = 1;

/// The value written to element `i` of the 64-bit arrays
fn element(i: usize) -> u64 {
    0x0101_0101_0101_0101 * (i as u64 + 1)
}

/// The 64-byte payload carrying `element(0..8)`
fn wide_payload() -> Vec<u8> {
    (0..8).flat_map(|i| element(i).to_le_bytes()).collect()
}

fn fd_node() -> Node<'static, Callbacks<'static>, CanFdMessage> {
    let od = object_dict4::get_od();
    Node::new(
        NodeId::new(FD_NODE_ID).unwrap(),
        Callbacks::default(),
        od.node_mbox(),
        od.node_state(),
        od,
    )
}

#[serial]
#[tokio::test]
async fn test_fd_node_uses_fd_frames() {
    let od = object_dict4::get_od();
    // The generated module exposes the frame type selected in the device config
    let _: Option<object_dict4::Frame> = None::<CanFdMessage>;
    let _: Option<object_dict1::Frame> = None::<zencan_common::can::CanMessage>;

    let mut bus = SimBus::<CanFdMessage>::new();
    bus.add_node(od.node_mbox());
    let mut node = fd_node();
    let mut client = get_fd_sdo_client(&mut bus, FD_NODE_ID);
    let mut rx = bus.new_receiver();

    let test_task = move |_ctx: TestContext| async move {
        // Expedited upload
        assert_eq!(1234, client.read_u32(0x1018, 1).await.unwrap());
        assert_eq!(12004, client.read_u32(0x1018, 2).await.unwrap());
        // Segmented upload
        assert_eq!(
            "Example 4 FD",
            client.read_visible_string(0x1008, 0).await.unwrap()
        );
        // Segmented download and readback
        let value = 0x1122_3344_5566_7788u64;
        client
            .download(0x2001, 0, &value.to_le_bytes())
            .await
            .unwrap();
        assert_eq!(
            value.to_le_bytes().to_vec(),
            client.upload(0x2001, 0).await.unwrap()
        );

        // Every SDO response was carried in an 8-byte CAN FD frame with bit rate switching
        let mut responses = 0;
        while let Some(frame) = rx.try_recv() {
            if frame.id() == CanId::std(0x580 + FD_NODE_ID as u16) {
                assert_eq!(8, frame.len());
                assert_eq!(8, frame.on_wire_len());
                assert!(frame.brs());
                responses += 1;
            }
        }
        assert!(responses > 0);
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[serial]
#[tokio::test]
async fn test_fd_tpdos() {
    let od = object_dict4::get_od();
    let mut bus = SimBus::<CanFdMessage>::new();
    bus.add_node(od.node_mbox());
    let mut node = fd_node();
    let mut nmt = NmtMaster::new(bus.new_classic_sender(), bus.new_classic_receiver());
    let mut rx = bus.new_receiver();

    let test_task = move |mut ctx: TestContext| async move {
        for i in 0..8 {
            od.object2000().set(i, element(i)).unwrap();
        }
        od.object2001().set_value(element(0));
        od.object2002().set_value(0xBEEF);

        nmt.nmt_start(0).await.unwrap();
        ctx.wait_for_process(1).await;
        rx.flush();

        // TPDO0: 64 bytes, a full CAN FD frame
        od.object2000().set_event_flag(1).unwrap();
        ctx.wait_for_process(1).await;
        let frame = rx.try_recv().expect("No TPDO0 received");
        assert!(rx.try_recv().is_none());
        assert_eq!(CanId::std(0x180 + FD_NODE_ID as u16), frame.id());
        assert_eq!(64, frame.len());
        assert_eq!(64, frame.on_wire_len());
        assert_eq!(wide_payload().as_slice(), frame.data());

        // TPDO1: 10 bytes, padded to 12 on the wire
        od.object2002().set_event_flag(0).unwrap();
        ctx.wait_for_process(1).await;
        let frame = rx.try_recv().expect("No TPDO1 received");
        assert_eq!(CanId::std(0x280 + FD_NODE_ID as u16), frame.id());
        assert_eq!(10, frame.len());
        assert_eq!(12, frame.on_wire_len());
        let mut expected = element(0).to_le_bytes().to_vec();
        expected.extend_from_slice(&[0xEF, 0xBE, 0, 0]);
        assert_eq!(expected.as_slice(), frame.padded_data());
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[serial]
#[tokio::test]
async fn test_fd_rpdos() {
    let od = object_dict4::get_od();
    let mut bus = SimBus::<CanFdMessage>::new();
    bus.add_node(od.node_mbox());
    let mut node = fd_node();
    let mut nmt = NmtMaster::new(bus.new_classic_sender(), bus.new_classic_receiver());
    let mut sender = bus.new_sender();

    let test_task = move |mut ctx: TestContext| async move {
        nmt.nmt_start(0).await.unwrap();
        ctx.wait_for_process(1).await;

        // RPDO0: 64 bytes
        let frame = CanFdMessage::new(CanId::std(0x200 + FD_NODE_ID as u16), &wide_payload());
        sender.send(frame).await.unwrap();
        ctx.wait_for_process(2).await;
        for i in 0..8 {
            assert_eq!(element(i), od.object2100().get(i).unwrap());
        }

        // RPDO1: 10 mapped bytes, received as a 12-byte frame. The padding is ignored.
        let mut data = element(7).to_le_bytes().to_vec();
        data.extend_from_slice(&[0x34, 0x12, 0, 0]);
        let frame = CanFdMessage::new(CanId::std(0x300 + FD_NODE_ID as u16), &data);
        sender.send(frame).await.unwrap();
        ctx.wait_for_process(2).await;
        assert_eq!(element(7), od.object2101().get_value());
        assert_eq!(0x1234, od.object2102().get_value());
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

/// A classic CAN node and a CAN FD node, generated in the same binary, sharing a CAN FD bus
///
/// The classic node models a node with a CAN FD tolerant controller: it only sees the frames which
/// fit in 8 bytes.
#[serial]
#[tokio::test]
async fn test_classic_and_fd_nodes_coexist() {
    let fd_od = object_dict4::get_od();
    let classic_od = object_dict1::get_od();

    let mut bus = SimBus::<CanFdMessage>::new();
    bus.add_node(fd_od.node_mbox());
    bus.add_classic_node(classic_od.node_mbox());

    let mut fd_node = fd_node();
    let mut classic_node = Node::new(
        NodeId::new(CLASSIC_NODE_ID).unwrap(),
        Callbacks::default(),
        classic_od.node_mbox(),
        classic_od.node_state(),
        classic_od,
    );
    let mut fd_client = get_fd_sdo_client(&mut bus, FD_NODE_ID);
    let mut classic_client = get_fd_sdo_client(&mut bus, CLASSIC_NODE_ID);
    let mut nmt = NmtMaster::new(bus.new_classic_sender(), bus.new_classic_receiver());
    let mut rx = bus.new_receiver();

    let test_task = move |mut ctx: TestContext| async move {
        // Both nodes answer SDO requests on the same bus
        assert_eq!(12004, fd_client.read_u32(0x1018, 2).await.unwrap());
        assert_eq!(12000, classic_client.read_u32(0x1018, 2).await.unwrap());

        for i in 0..8 {
            fd_od.object2000().set(i, element(i)).unwrap();
        }
        classic_client.write_u32(0x2000, 1, 0xCAFE).await.unwrap();

        nmt.nmt_start(0).await.unwrap();
        ctx.wait_for_process(1).await;
        rx.flush();

        // The FD node sends its 64-byte TPDO0; the classic node sends TPDO1 (0x2000sub1, 4 bytes)
        fd_od.object2000().set_event_flag(1).unwrap();
        classic_od.object2000().set_event_flag(1).unwrap();
        ctx.wait_for_process(1).await;

        let mut fd_tpdo = None;
        let mut classic_tpdo = None;
        while let Some(frame) = rx.try_recv() {
            if frame.id() == CanId::std(0x180 + FD_NODE_ID as u16) {
                fd_tpdo = Some(frame);
            } else if frame.id() == CanId::std(0x200 + CLASSIC_NODE_ID as u16) {
                classic_tpdo = Some(frame);
            }
        }
        assert_eq!(wide_payload().as_slice(), fd_tpdo.unwrap().data());
        assert_eq!(&0xCAFEu32.to_le_bytes(), classic_tpdo.unwrap().data());

        // The classic node keeps working after the 64-byte frame went past it
        assert_eq!(0xCAFE, classic_client.read_u32(0x2000, 1).await.unwrap());
    };

    test_with_background_process(&mut [&mut fd_node, &mut classic_node], &mut bus, test_task).await;
}

/// A CAN FD master may pad every frame, NMT and SYNC included, beyond its significant bytes
#[serial]
#[tokio::test]
async fn test_fd_padded_nmt_and_sync() {
    let od = object_dict4::get_od();
    let mut bus = SimBus::<CanFdMessage>::new();
    bus.add_node(od.node_mbox());
    let mut node = fd_node();
    let mut client = get_fd_sdo_client(&mut bus, FD_NODE_ID);
    let mut sender = bus.new_sender();
    let mut rx = bus.new_receiver();

    let test_task = move |mut ctx: TestContext| async move {
        // Make TPDO1 synchronous, sent on every SYNC
        client.download(0x1801, 2, &[1]).await.unwrap();
        od.object2002().set_value(0xBEEF);

        // NMT Start for all nodes, padded to a 64-byte frame
        let mut nmt_start = [0u8; 64];
        nmt_start[0] = 1;
        sender
            .send(CanFdMessage::new(CanId::std(0), &nmt_start))
            .await
            .unwrap();
        ctx.wait_for_process(2).await;

        // An RPDO is only received in Operational, so this one shows the node was started
        let mut data = element(7).to_le_bytes().to_vec();
        data.extend_from_slice(&[0x34, 0x12, 0, 0]);
        sender
            .send(CanFdMessage::new(
                CanId::std(0x300 + FD_NODE_ID as u16),
                &data,
            ))
            .await
            .unwrap();
        ctx.wait_for_process(2).await;
        assert_eq!(0x1234, od.object2102().get_value());

        // SYNC padded to a 64-byte frame triggers the synchronous TPDO
        rx.flush();
        sender
            .send(CanFdMessage::new(CanId::std(0x80), &[0; 64]))
            .await
            .unwrap();
        ctx.wait_for_process(2).await;
        let tpdo1 = CanId::std(0x280 + FD_NODE_ID as u16);
        let mut tpdos = 0;
        while let Some(frame) = rx.try_recv() {
            if frame.id() == tpdo1 {
                tpdos += 1;
            }
        }
        assert_eq!(1, tpdos);
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}
