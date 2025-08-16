use std::{
    convert::Infallible,
    sync::{Arc, RwLock},
    time::Duration,
};

use integration_tests::object_dict1;
use zencan_client::{RawAbortCode, SdoClientError};
use zencan_common::sdo::AbortCode;

mod utils;
use utils::{setup_single_node, test_with_background_process, BusLogger};

#[serial_test::serial]
#[tokio::test]
async fn test_device_info_readback() {
    const DEVICE_NAME_ID: u16 = 0x1008;
    const DEVICE_HW_VER_ID: u16 = 0x1009;
    const DEVICE_SW_VER_ID: u16 = 0x100A;

    let (mut node, mut client, mut bus) = setup_single_node(
        &object_dict1::OD_TABLE,
        &object_dict1::NODE_MBOX,
        &object_dict1::NODE_STATE,
    );

    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = async move {
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

    test_with_background_process(&mut [&mut node], &mut bus.new_sender(), test_task).await;
}

#[serial_test::serial]
#[tokio::test]
async fn test_identity_readback() {
    const IDENTITY_OBJECT_ID: u16 = 0x1018;
    const VENDOR_SUB_ID: u8 = 1;
    const PRODUCT_SUB_ID: u8 = 2;
    const REVISION_SUB_ID: u8 = 3;
    const SERIAL_SUB_ID: u8 = 4;

    let (mut node, mut client, mut bus) = setup_single_node(
        &object_dict1::OD_TABLE,
        &object_dict1::NODE_MBOX,
        &object_dict1::NODE_STATE,
    );

    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = async move {
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

    let mut sender = bus.new_sender();
    test_with_background_process(&mut [&mut node], &mut sender, test_task).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_string_write() {
    let (mut node, mut client, mut bus) = setup_single_node(
        &object_dict1::OD_TABLE,
        &object_dict1::NODE_MBOX,
        &object_dict1::NODE_STATE,
    );
    let mut sender = bus.new_sender();
    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = async move {
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

    test_with_background_process(&mut [&mut node], &mut sender, test_task).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_record_access() {
    const OBJECT_ID: u16 = 0x2001;

    let od = &object_dict1::OD_TABLE;
    let state = &object_dict1::NODE_STATE;
    let mbox = &object_dict1::NODE_MBOX;
    let (mut node, mut client, mut bus) = setup_single_node(od, mbox, state);

    // Create a logger to display messages on the bus on test failure for debugging
    let _logger = BusLogger::new(bus.new_receiver());

    let mut sender = bus.new_sender();

    let test_task = async move {
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

    test_with_background_process(&mut [&mut node], &mut sender, test_task).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_array_access() {
    const OBJECT_ID: u16 = 0x2000;

    let (mut node, mut client, mut bus) = setup_single_node(
        &object_dict1::OD_TABLE,
        &object_dict1::NODE_MBOX,
        &object_dict1::NODE_STATE,
    );
    let mut sender = bus.new_sender();

    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = async move {
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

    test_with_background_process(&mut [&mut node], &mut sender, test_task).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_store_and_restore_objects() {
    let _ = env_logger::try_init();

    const SAVE_CMD: u32 = 0x73617665;

    let od = &object_dict1::OD_TABLE;
    let (mut node, mut client, mut bus) =
        setup_single_node(od, &object_dict1::NODE_MBOX, &object_dict1::NODE_STATE);

    let mut sender = bus.new_sender();

    let _logger = BusLogger::new(bus.new_receiver());

    let serialized_data = Arc::new(RwLock::new(Vec::new()));
    let cloned_data = serialized_data.clone();
    let store_objects_callback = Box::leak(Box::new(
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
        },
    ));
    node.register_store_objects(store_objects_callback);

    let test_task = async move {
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

        tokio::time::sleep(Duration::from_millis(5)).await;

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

        zencan_node::restore_stored_objects(od, &serialized_data.read().unwrap());

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

    test_with_background_process(&mut [&mut node], &mut sender, test_task).await;
}

#[serial_test::serial]
#[tokio::test]
async fn test_empty_string_read() {
    let _ = env_logger::try_init();

    let od = &object_dict1::OD_TABLE;
    let (mut node, mut client, mut bus) =
        setup_single_node(od, &object_dict1::NODE_MBOX, &object_dict1::NODE_STATE);

    let mut sender = bus.new_sender();

    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = async move {
        let empty_string = client.upload_utf8(0x3005, 0).await.unwrap();
        assert_eq!("", empty_string);
    };
    test_with_background_process(&mut [&mut node], &mut sender, test_task).await;
}
