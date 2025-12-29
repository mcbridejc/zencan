use std::{
    convert::Infallible,
    sync::{Arc, RwLock},
};

use integration_tests::{object_dict1, prelude::*};
use serial_test::serial;
use zencan_client::nmt_master::NmtMaster;

#[serial]
#[tokio::test]
async fn test_device_info_readback() {
    use object_dict1::*;
    const DEVICE_NAME_ID: u16 = 0x1008;
    const DEVICE_HW_VER_ID: u16 = 0x1009;
    const DEVICE_SW_VER_ID: u16 = 0x100A;
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
        assert_eq!(
            &client.read_utf8(DEVICE_NAME_ID, 0).await.unwrap(),
            "Example 1"
        );
        assert_eq!(
            &client.read_utf8(DEVICE_HW_VER_ID, 0).await.unwrap(),
            "v1.2.3"
        );
        assert_eq!(
            &client.read_utf8(DEVICE_SW_VER_ID, 0).await.unwrap(),
            "v2.1.0"
        );
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[serial]
#[tokio::test]
async fn test_identity_readback() {
    use object_dict1::*;
    const IDENTITY_OBJECT_ID: u16 = 0x1018;
    const VENDOR_SUB_ID: u8 = 1;
    const PRODUCT_SUB_ID: u8 = 2;
    const REVISION_SUB_ID: u8 = 3;
    const SERIAL_SUB_ID: u8 = 4;
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
        // Check that the identity matches the values defined in the example1.toml device config
        assert_eq!(
            client
                .read_u32(IDENTITY_OBJECT_ID, VENDOR_SUB_ID)
                .await
                .unwrap(),
            1234,
        );
        assert_eq!(
            client
                .read_u32(IDENTITY_OBJECT_ID, PRODUCT_SUB_ID)
                .await
                .unwrap(),
            12000,
        );
        assert_eq!(
            client
                .read_u32(IDENTITY_OBJECT_ID, REVISION_SUB_ID)
                .await
                .unwrap(),
            1,
        );
        assert_eq!(
            client
                .read_u32(IDENTITY_OBJECT_ID, SERIAL_SUB_ID)
                .await
                .unwrap(),
            0,
        );
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[tokio::test]
#[serial]
async fn test_string_write() {
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
        // Transfer a string short enough to be done expedited
        client.download(0x2002, 0, "Test".as_bytes()).await.unwrap();
        let readback = client.upload(0x2002, 0).await.unwrap();
        assert_eq!("Test".as_bytes(), readback);
        // Transfer a longer string which will do segmented transfer
        client
            .download(0x2002, 0, "Testers".as_bytes())
            .await
            .unwrap();
        let readback = client.upload(0x2002, 0).await.unwrap();
        assert_eq!("Testers".as_bytes(), readback);
        // Transfer an even longer string which will do segmented transfer with two segments
        client
            .download(0x2002, 0, "Testers123".as_bytes())
            .await
            .unwrap();
        let readback = client.upload(0x2002, 0).await.unwrap();
        assert_eq!("Testers123".as_bytes(), readback);
        // Transfer as max-length string (the default value in EDS is 11 characters long)
        client
            .download(0x2002, 0, "Testers1234".as_bytes())
            .await
            .unwrap();
        let readback = client.upload(0x2002, 0).await.unwrap();
        assert_eq!("Testers1234".as_bytes(), readback);
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[tokio::test]
#[serial]
async fn test_record_access() {
    use object_dict1::*;
    const OBJECT_ID: u16 = 0x2001;
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

    // Create a logger to display messages on the bus on test failure for debugging
    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = move |_ctx| async move {
        let size_data = client.upload(OBJECT_ID, 0).await.unwrap();
        assert_eq!(1, size_data.len());
        assert_eq!(4, size_data[0]); // Highest sub index supported

        // Check default values of read-only subs
        let sub3_bytes = client.upload(OBJECT_ID, 3).await.unwrap();
        assert_eq!(2, sub3_bytes.len());
        assert_eq!(0x20, u16::from_le_bytes(sub3_bytes.try_into().unwrap()));

        // Write/readback sub1
        client
            .download(OBJECT_ID, 1, &4567u32.to_le_bytes())
            .await
            .unwrap();
        let sub1_bytes = client.upload(OBJECT_ID, 1).await.unwrap();
        assert_eq!(4567, u32::from_le_bytes(sub1_bytes.try_into().unwrap()));

        // Sub3 is read-only; writing should return an abort
        let res = client.download(OBJECT_ID, 3, &100u16.to_le_bytes()).await;
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            SdoClientError::ServerAbort {
                index: OBJECT_ID,
                sub: 3,
                abort_code: RawAbortCode::Valid(AbortCode::ReadOnly)
            }
        );
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[tokio::test]
#[serial]
async fn test_array_access() {
    use object_dict1::*;
    const OBJECT_ID: u16 = 0x2000;
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
        let size_data = client.upload(OBJECT_ID, 0).await.unwrap();
        assert_eq!(1, size_data.len());
        assert_eq!(2, size_data[0]); // Highest sub index supported

        // Write and read
        client
            .download(OBJECT_ID, 1, &(-40i32).to_le_bytes())
            .await
            .unwrap();
        let data = client.upload(OBJECT_ID, 1).await.unwrap();
        assert_eq!(-40, i32::from_le_bytes(data.try_into().unwrap()));

        client
            .download(OBJECT_ID, 2, &(99i32).to_le_bytes())
            .await
            .unwrap();
        let data = client.upload(OBJECT_ID, 2).await.unwrap();
        assert_eq!(99, i32::from_le_bytes(data.try_into().unwrap()));
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[tokio::test]
#[serial]
async fn test_store_and_restore_objects() {
    use object_dict1::*;
    const NODE_ID: u8 = 1;
    const SAVE_CMD: u32 = 0x73617665;

    let serialized_data = Arc::new(RwLock::new(Vec::new()));
    let cloned_data = serialized_data.clone();
    let mut store_objects_callback =
        move |reader: &mut dyn embedded_io::Read<Error = Infallible>, _size: usize| {
            let mut buf = [0; 32];
            loop {
                let n = reader.read(&mut buf).unwrap();
                let mut data = cloned_data.write().unwrap();
                data.extend_from_slice(&buf[..n]);
                if n < 32 {
                    break;
                }
            }
        };

    let mut bus = SimBus::new();
    bus.add_node(&NODE_MBOX);
    let mut callbacks = Callbacks::new();
    callbacks.store_objects = Some(&mut store_objects_callback);

    let mut node = Node::new(
        NodeId::new(NODE_ID).unwrap(),
        callbacks,
        &NODE_MBOX,
        &NODE_STATE,
        &OD_TABLE,
    );
    let mut client = get_sdo_client(&mut bus, NODE_ID);

    let _ = env_logger::try_init();
    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = move |mut ctx: TestContext| async move {
        // Load some values to persist
        client
            .download(0x2002, 0, "SAVEME".as_bytes())
            .await
            .unwrap();
        client
            .download(0x2003, 0, "SAVEME".as_bytes())
            .await
            .unwrap();
        client.download_u32(0x2000, 1, 900).await.unwrap();

        // Trigger a save
        client.download_u32(0x1010, 1, SAVE_CMD).await.unwrap();

        ctx.wait_for_process(1).await;

        assert!(!serialized_data.read().unwrap().is_empty());

        // Change the values
        client
            .download(0x2002, 0, "NOTSAVED".as_bytes())
            .await
            .unwrap();
        client
            .download(0x2003, 0, "NOTSAVED".as_bytes())
            .await
            .unwrap();
        client.download_u32(0x2000, 1, 500).await.unwrap();

        zencan_node::restore_stored_objects(&OD_TABLE, &serialized_data.read().unwrap());

        // 0x2002 has persist set, so should have been saved
        assert_eq!(client.upload(0x2002, 0).await.unwrap(), "SAVEME".as_bytes());
        // should not have saved
        assert_eq!(
            client.upload(0x2003, 0).await.unwrap(),
            "NOTSAVED".as_bytes()
        );
        // Should have saved
        assert_eq!(client.upload_u32(0x2000, 1).await.unwrap(), 900);
    };

    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

#[serial]
#[tokio::test]
async fn test_empty_string_read() {
    use object_dict1::*;

    let _ = env_logger::try_init();

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
        let empty_string = client.upload_utf8(0x3005, 0).await.unwrap();
        assert_eq!("", empty_string);
    };
    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}

/// Verify that the NODE calls the appropriate state change callbacks
#[serial]
#[tokio::test]
async fn test_node_state_callbacks() {
    use object_dict1::*;

    let _ = env_logger::try_init();

    const NODE_ID: u8 = 1;
    let mut bus = SimBus::new();
    bus.add_node(&NODE_MBOX);
    let mut callbacks = Callbacks::new();
    let _logger = BusLogger::new(bus.new_receiver());

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum LastCallback {
        ResetApp,
        ResetComms,
        Preop,
        Operational,
        Stopped,
    }

    let (callback_tx, callback_rx) = std::sync::mpsc::channel();
    let mut reset_app = |_| {
        callback_tx.send(LastCallback::ResetApp).unwrap();
    };
    let mut reset_comms = |_| {
        callback_tx.send(LastCallback::ResetComms).unwrap();
    };
    let mut enter_preop = |_| {
        callback_tx.send(LastCallback::Preop).unwrap();
    };
    let mut enter_operational = |_| {
        callback_tx.send(LastCallback::Operational).unwrap();
    };
    let mut enter_stopped = |_| {
        callback_tx.send(LastCallback::Stopped).unwrap();
    };
    callbacks.reset_app = Some(&mut reset_app);
    callbacks.reset_comms = Some(&mut reset_comms);
    callbacks.enter_preoperational = Some(&mut enter_preop);
    callbacks.enter_operational = Some(&mut enter_operational);
    callbacks.enter_stopped = Some(&mut enter_stopped);

    let mut node = Node::new(
        NodeId::new(NODE_ID).unwrap(),
        callbacks,
        &NODE_MBOX,
        &NODE_STATE,
        &OD_TABLE,
    );
    let mut nmt_master = NmtMaster::new(bus.new_sender(), bus.new_receiver());

    let test_task = move |mut ctx: TestContext| async move {
        // Reset app should be called during bootup
        assert_eq!(callback_rx.try_recv().unwrap(), LastCallback::ResetApp);
        // Enter Preop right after
        assert_eq!(callback_rx.try_recv().unwrap(), LastCallback::Preop);
        // No more
        assert!(callback_rx.try_recv().is_err());

        nmt_master.nmt_start(NODE_ID).await.unwrap();
        ctx.wait_for_process(1).await;

        assert_eq!(callback_rx.try_recv().unwrap(), LastCallback::Operational);

        nmt_master.nmt_stop(NODE_ID).await.unwrap();
        ctx.wait_for_process(1).await;

        assert_eq!(callback_rx.try_recv().unwrap(), LastCallback::Stopped);

        nmt_master.nmt_reset_comms(NODE_ID).await.unwrap();
        ctx.wait_for_process(2).await;

        assert_eq!(callback_rx.try_recv().unwrap(), LastCallback::ResetComms);
        assert_eq!(callback_rx.try_recv().unwrap(), LastCallback::Preop);
        assert!(callback_rx.try_recv().is_err());
    };
    test_with_background_process(&mut [&mut node], &mut bus, test_task).await;
}
