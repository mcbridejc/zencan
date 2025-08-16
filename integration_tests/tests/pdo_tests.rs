//! Test node PDO operations
//!

use std::time::Duration;

use integration_tests::{
    object_dict1,
    sim_bus::{SimBus, SimBusReceiver, SimBusSender},
};
use serial_test::serial;
use tokio::time::timeout;
use zencan_client::{nmt_master::NmtMaster, PdoConfig, PdoMapping, SdoClient};
use zencan_common::{
    messages::{CanId, CanMessage, SyncObject},
    traits::{AsyncCanReceiver, AsyncCanSender},
    NodeId,
};
use zencan_node::object_dict::{find_object, ODEntry};
use zencan_node::{Node, NodeMbox, NodeStateAccess};

mod utils;
use utils::{test_with_background_process, BusLogger};

fn setup<'a, S: NodeStateAccess>(
    od: &'static [ODEntry],
    mbox: &'static NodeMbox,
    state: &'static S,
) -> (
    Node,
    SdoClient<SimBusSender<'a>, SimBusReceiver>,
    SimBus<'a>,
) {
    const SLAVE_NODE_ID: u8 = 1;

    let node = Node::init(NodeId::new(SLAVE_NODE_ID).unwrap(), mbox, state, od).finalize();

    let mut bus = SimBus::new(vec![mbox]);

    let sender = bus.new_sender();
    let receiver = bus.new_receiver();
    let client = SdoClient::new_std(SLAVE_NODE_ID, sender, receiver);

    (node, client, bus)
}

#[tokio::test]
#[serial]
async fn test_rpdo_assignment() {
    let od = &object_dict1::OD_TABLE;
    let state = &object_dict1::NODE_STATE;
    let mbox = &object_dict1::NODE_MBOX;

    let (mut node, mut client, mut bus) = setup(od, mbox, state);
    let mut sender = bus.new_sender();
    let rx = bus.new_receiver();

    let mut nmt = NmtMaster::new(bus.new_sender(), bus.new_receiver());

    let _bus_logger = BusLogger::new(rx);

    let mut pdo_sender = bus.new_sender();

    let test_task = async move {
        // Readback the largest sub index
        assert_eq!(2, client.upload_u8(0x1400, 0).await.unwrap());

        // Set COB-ID and readback
        // Invalid bit cleared, and ID == 0x201.
        let cob_id_word: u32 = 0x201;
        client.download_u32(0x1400, 1, cob_id_word).await.unwrap();

        let readback_cob_id_word = client.upload_u32(0x1400, 1).await.unwrap();
        assert_eq!(cob_id_word, readback_cob_id_word);

        // Set RPDO1 to map to object 0x2000, subindex 1, length 32 bits
        let mapping_entry: u32 = (0x2000 << 16) | (1 << 8) | 32;
        client.download_u32(0x1600, 1, mapping_entry).await.unwrap();
        // Set the number of valid mappings
        client.download_u8(0x1600, 0, 1).await.unwrap();

        // Put in operational mode
        nmt.nmt_start(0).await.unwrap();

        // Now send a PDO message and it should update the mapped object
        pdo_sender
            .send(CanMessage::new(CanId::Std(0x201), &500u32.to_le_bytes()))
            .await
            .unwrap();

        // Delay a bit, because node process() method has to be called for PDO to apply
        tokio::time::sleep(Duration::from_millis(10)).await;
        // Readback the mapped object; the PDO message above should have updated it
        assert_eq!(500, client.upload_u32(0x2000, 1).await.unwrap());
    };

    test_with_background_process(&mut [&mut node], &mut sender, test_task).await;
}

#[tokio::test]
#[serial]
async fn test_tpdo_asignment() {
    let od = &integration_tests::object_dict1::OD_TABLE;
    let state = &integration_tests::object_dict1::NODE_STATE;
    let mbox = &integration_tests::object_dict1::NODE_MBOX;
    let (mut node, mut client, mut bus) = setup(od, mbox, state);

    let _logger = BusLogger::new(bus.new_receiver());

    let mut rx = bus.new_receiver();

    // Set COB-ID
    const TPDO_COMM1_ID: u16 = 0x1800;
    const PDO_COMM_COB_SUBID: u8 = 1;
    const PDO_COMM_TRANSMISSION_TYPE_SUBID: u8 = 2;

    let mut nmt = NmtMaster::new(bus.new_sender(), bus.new_receiver());

    let mut sender = bus.new_sender();
    let test_task = async move {
        // Set the TPDO COB ID
        client
            .download(TPDO_COMM1_ID, PDO_COMM_COB_SUBID, &0x181u32.to_le_bytes())
            .await
            .unwrap();
        // Set to sync driven
        client
            .download(
                TPDO_COMM1_ID,
                PDO_COMM_TRANSMISSION_TYPE_SUBID,
                &1u8.to_le_bytes(),
            )
            .await
            .unwrap();

        client.download_u32(0x2000, 1, 222).await.unwrap();
        client.download_u32(0x2001, 1, 333).await.unwrap();

        // Set the TPDO mapping to 0x2000, subindex 1, length 32 bits
        let mapping_entry: u32 = (0x2000 << 16) | (1 << 8) | 32;
        client.download_u32(0x1A00, 1, mapping_entry).await.unwrap();
        // Set the second TPDO mapping entry to 0x2001, subindex 1, length 32 bits
        let mapping_entry: u32 = (0x2001 << 16) | (1 << 8) | 32;
        client.download_u32(0x1A00, 2, mapping_entry).await.unwrap();
        // Set the number of valid mappings
        client.download_u8(0x1A00, 0, 2).await.unwrap();
        // Node has to be in Operating mode to send PDOs
        nmt.nmt_start(0).await.unwrap();

        rx.flush();

        let sync_msg = SyncObject::new(1).into();
        sender.send(sync_msg).await.unwrap();

        // We expect to receive the sync message just sent first
        let rx_sync_msg = timeout(Duration::from_millis(50), rx.recv())
            .await
            .expect("Expected SYNC message, no CAN message received")
            .expect("recv returned an error");
        assert_eq!(sync_msg.id, rx_sync_msg.id);
        // Then expect a PDO message
        let msg = timeout(Duration::from_millis(50), rx.recv())
            .await
            .expect("Expected PDO, no CAN message received")
            .expect("recv returned an error");

        assert_eq!(CanId::std(0x181), msg.id);
        let field1 = u32::from_le_bytes(msg.data[0..4].try_into().unwrap());
        let field2 = u32::from_le_bytes(msg.data[4..8].try_into().unwrap());
        assert_eq!(222, field1);
        assert_eq!(333, field2);
    };

    // Create a second sender to pass to the test processer since the previous got moved into
    // test_task
    let mut sender = bus.new_sender();

    test_with_background_process(&mut [&mut node], &mut sender, test_task).await;
}

#[serial]
#[tokio::test]
async fn test_tpdo_event_flags() {
    let od = &integration_tests::object_dict1::OD_TABLE;
    let state = &integration_tests::object_dict1::NODE_STATE;
    let mbox = &integration_tests::object_dict1::NODE_MBOX;
    let (mut node, mut client, mut bus) = setup(od, mbox, state);

    let _logger = BusLogger::new(bus.new_receiver());

    // Set COB-ID
    const TPDO_COMM1_ID: u16 = 0x1800;
    const PDO_COMM_COB_SUBID: u8 = 1;
    const PDO_COMM_TRANSMISSION_TYPE_SUBID: u8 = 2;

    let mut rx = bus.new_receiver();

    let mut nmt = NmtMaster::new(bus.new_sender(), bus.new_receiver());

    let test_task = async move {
        // Set the TPDO COB ID
        client
            .download(TPDO_COMM1_ID, PDO_COMM_COB_SUBID, &0x181u32.to_le_bytes())
            .await
            .unwrap();
        // Set to asynchronous transmission
        client
            .download(
                TPDO_COMM1_ID,
                PDO_COMM_TRANSMISSION_TYPE_SUBID,
                &254u8.to_le_bytes(),
            )
            .await
            .unwrap();

        // Set some known values into some application objects
        client.download_u32(0x2000, 1, 222).await.unwrap();
        client.download_u32(0x2001, 1, 333).await.unwrap();

        // Set the first TPDO mapping to 0x2000, subindex 1, length 32 bits
        let mapping_entry: u32 = (0x2000 << 16) | (1 << 8) | 32;
        client
            .download(0x1A00, 1, &mapping_entry.to_le_bytes())
            .await
            .unwrap();
        // Set the second TPDO mapping entry to 0x2001, subindex 1, length 32 bits
        let mapping_entry: u32 = (0x2001 << 16) | (1 << 8) | 32;
        client
            .download(0x1A00, 2, &mapping_entry.to_le_bytes())
            .await
            .unwrap();

        // Node has to be in Operating mode to send PDOs
        nmt.nmt_start(0).await.unwrap();

        rx.flush();

        tokio::time::sleep(Duration::from_millis(5)).await;

        // No messages in queue
        assert!(rx.try_recv().is_none());

        let obj = find_object(od, 0x2000).expect("Could not find object 0x2000");
        // Set the event flag for sub 1
        obj.set_event_flag(1).expect("Error setting event flag");

        tokio::time::sleep(Duration::from_millis(5)).await;
        let _pdomsg = rx.try_recv().expect("No message received after TPDO event");
        // should only have gotten one message
        assert!(rx.try_recv().is_none());
    };

    test_with_background_process(&mut [&mut node], &mut bus.new_sender(), test_task).await;
}

#[serial]
#[tokio::test]
async fn test_pdo_configuration() {
    let od = &integration_tests::object_dict1::OD_TABLE;
    let state = &integration_tests::object_dict1::NODE_STATE;
    let mbox = &integration_tests::object_dict1::NODE_MBOX;
    let (mut node, mut client, mut bus) = setup(od, mbox, state);

    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = async move {
        let config = PdoConfig {
            cob: 0x301,
            enabled: true,
            mappings: vec![
                PdoMapping {
                    index: 0x2000,
                    sub: 1,
                    size: 32,
                },
                PdoMapping {
                    index: 0x2001,
                    sub: 1,
                    size: 32,
                },
            ],
            transmission_type: 254,
        };

        client.configure_tpdo(0, &config).await?;

        // Check that the expected objects got the expected values
        assert_eq!(2, client.upload_u8(0x1A00, 0).await?);
        assert_eq!(
            (0x2000 << 16) | 1 << 8 | 32,
            client.upload_u32(0x1A00, 1).await?
        );
        assert_eq!(
            (0x2001 << 16) | 1 << 8 | 32,
            client.upload_u32(0x1A00, 2).await?
        );
        assert_eq!(254, client.upload_u8(0x1800, 2).await?);
        assert_eq!(0x301, client.upload_u32(0x1800, 1).await?);

        Ok::<_, Box<dyn std::error::Error>>(())
    };

    let result =
        test_with_background_process(&mut [&mut node], &mut bus.new_sender(), test_task).await;

    if let Err(e) = result {
        panic!("{}", e);
    }
}
