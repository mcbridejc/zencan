//! Test node PDO operations
//!

use std::time::Duration;

use integration_tests::{object_dict1, prelude::*};
use serial_test::serial;
use tokio::time::timeout;
use zencan_client::nmt_master::NmtMaster;
use zencan_common::{
    messages::{CanId, CanMessage, SyncObject},
    node_configuration::PdoConfig,
    pdo::PdoMapping,
    traits::{AsyncCanReceiver, AsyncCanSender},
    NodeId,
};
use zencan_node::object_dict::find_object;

#[serial]
#[tokio::test]
async fn test_rpdo_assignment() {
    use object_dict1::*;
    const NODE_ID: u8 = 1;

    let mut bus = SimBus::new();
    bus.add_node(&NODE_MBOX);
    let callbacks = Callbacks::new();
    let mut node = Node::new(
        NodeId::new(NODE_ID).unwrap(),
        callbacks,
        &NODE_MBOX,
        &NODE_STATE,
        &OD_TABLE,
    );
    let mut client = get_sdo_client(&mut bus, NODE_ID);

    let rx = bus.new_receiver();

    let mut nmt = NmtMaster::new(bus.new_sender(), bus.new_receiver());

    let _bus_logger = BusLogger::new(rx);

    let mut pdo_sender = bus.new_sender();

    let test_task = move |mut ctx: TestContext| async move {
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
        ctx.wait_for_process(1).await;
        // Readback the mapped object; the PDO message above should have updated it
        assert_eq!(500, client.upload_u32(0x2000, 1).await.unwrap());
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[serial]
#[tokio::test]
async fn test_tpdo_assignment() {
    use object_dict1::*;
    const NODE_ID: u8 = 1;

    let mut bus = SimBus::new();
    bus.add_node(&NODE_MBOX);
    let callbacks = Callbacks::new();
    let mut node = Node::new(
        NodeId::new(NODE_ID).unwrap(),
        callbacks,
        &NODE_MBOX,
        &NODE_STATE,
        &OD_TABLE,
    );
    let mut client = get_sdo_client(&mut bus, NODE_ID);

    let _logger = BusLogger::new(bus.new_receiver());

    let mut rx = bus.new_receiver();

    const TPDO_COMM1_ID: u16 = 0x1800;
    const PDO_COMM_COB_SUBID: u8 = 1;
    const PDO_COMM_TRANSMISSION_TYPE_SUBID: u8 = 2;

    let mut nmt = NmtMaster::new(bus.new_sender(), bus.new_receiver());

    let mut sender = bus.new_sender();
    let test_task = move |_ctx| async move {
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

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[serial]
#[tokio::test]
async fn test_tpdo_event_flags() {
    use object_dict1::*;
    const NODE_ID: u8 = 1;

    let mut bus = SimBus::new();
    bus.add_node(&NODE_MBOX);
    let callbacks = Callbacks::new();
    let mut node = Node::new(
        NodeId::new(NODE_ID).unwrap(),
        callbacks,
        &NODE_MBOX,
        &NODE_STATE,
        &OD_TABLE,
    );
    let mut client = get_sdo_client(&mut bus, NODE_ID);

    let _logger = BusLogger::new(bus.new_receiver());

    // Set COB-ID
    const TPDO_COMM1_ID: u16 = 0x1800;
    const PDO_COMM_COB_SUBID: u8 = 1;
    const PDO_COMM_TRANSMISSION_TYPE_SUBID: u8 = 2;

    let mut rx = bus.new_receiver();

    let mut nmt = NmtMaster::new(bus.new_sender(), bus.new_receiver());

    let test_task = move |mut ctx: TestContext| async move {
        // Configure TPDO0 to send data
        client
            .configure_tpdo(
                0,
                &PdoConfig {
                    cob_id: CanId::std(0x181),
                    enabled: true,
                    rtr_disabled: false,
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
                },
            )
            .await
            .unwrap();

        // Disable TPDO1 (which is on by default in example system)
        client
            .configure_tpdo(
                1,
                &PdoConfig {
                    cob_id: CanId::std(0),
                    enabled: false,
                    rtr_disabled: false,
                    mappings: vec![],
                    transmission_type: 0,
                },
            )
            .await
            .unwrap();

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

        ctx.wait_for_process(1).await;

        // No messages in queue
        assert!(rx.try_recv().is_none());

        let obj = find_object(&OD_TABLE, 0x2000).expect("Could not find object 0x2000");
        // Set the event flag for sub 1
        obj.set_event_flag(1).expect("Error setting event flag");

        ctx.wait_for_process(1).await;

        let _pdomsg = rx.try_recv().expect("No message received after TPDO event");
        // should only have gotten one message
        assert!(rx.try_recv().is_none());
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[serial]
#[tokio::test]
async fn test_pdo_configuration() {
    use object_dict1::*;
    const NODE_ID: u8 = 1;

    let mut bus = SimBus::new();
    bus.add_node(&NODE_MBOX);
    let callbacks = Callbacks::new();
    let mut node = Node::new(
        NodeId::new(NODE_ID).unwrap(),
        callbacks,
        &NODE_MBOX,
        &NODE_STATE,
        &OD_TABLE,
    );
    let mut client = get_sdo_client(&mut bus, NODE_ID);

    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = move |_ctx| async move {
        let config = PdoConfig {
            cob_id: CanId::std(0x301),
            enabled: true,
            rtr_disabled: true,
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

    let result = test_with_background_process(&mut [&mut node], &mut bus, test_task).await;

    if let Err(e) = result {
        panic!("{}", e);
    }
}

/// Check that the PDOs have the default values defined in example1.toml after node init
#[serial]
#[tokio::test]
async fn test_pdo_defaults() {
    use object_dict1::*;
    const NODE_ID: u8 = 1;

    let mut bus = SimBus::new();
    bus.add_node(&NODE_MBOX);
    let callbacks = Callbacks::new();
    let mut node = Node::new(
        NodeId::new(NODE_ID).unwrap(),
        callbacks,
        &NODE_MBOX,
        &NODE_STATE,
        &OD_TABLE,
    );
    let mut client = get_sdo_client(&mut bus, NODE_ID);
    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = move |_ctx| async move {
        // Check that the initial value matches the one defined in example1.toml
        let tpdo1_cfg = client.read_tpdo_config(1).await.unwrap();
        assert_eq!(true, tpdo1_cfg.enabled);
        assert_eq!(CanId::std(0x201), tpdo1_cfg.cob_id);
        assert_eq!(254, tpdo1_cfg.transmission_type);
        assert_eq!(1, tpdo1_cfg.mappings.len());
        assert_eq!(0x2000, tpdo1_cfg.mappings[0].index);
        assert_eq!(1, tpdo1_cfg.mappings[0].sub);
        assert_eq!(32, tpdo1_cfg.mappings[0].size);

        let rpdo0_cfg = client.read_rpdo_config(0).await.unwrap();
        assert_eq!(true, rpdo0_cfg.enabled);
        assert_eq!(CanId::std(0x300), rpdo0_cfg.cob_id);
        assert_eq!(254, rpdo0_cfg.transmission_type);
        assert_eq!(1, rpdo0_cfg.mappings.len());
        assert_eq!(0x2000, rpdo0_cfg.mappings[0].index);
        assert_eq!(2u8, rpdo0_cfg.mappings[0].sub);
        assert_eq!(32, rpdo0_cfg.mappings[0].size);
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}
