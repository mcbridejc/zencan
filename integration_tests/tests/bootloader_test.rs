mod utils;

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use utils::setup_single_node;
use zencan_common::constants::values::{BOOTLOADER_ERASE_CMD, BOOTLOADER_RESET_CMD};
use zencan_node::BootloaderSectionCallbacks;

use crate::utils::{test_with_background_process, BusLogger};

use integration_tests::{object_dict2, object_dict3};

const BOOTLOADER_INFO_INDEX: u16 = 0x5500;
const BOOTLOADER_SECTION0_INDEX: u16 = 0x5510;

#[serial_test::serial]
#[tokio::test]
async fn test_device_info_readback() {
    let (mut node, mut client, mut bus) = setup_single_node(
        &object_dict2::OD_TABLE,
        &object_dict2::NODE_MBOX,
        &object_dict2::NODE_STATE,
    );

    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = async move {
        // Highest sub index
        assert_eq!(3, client.read_u8(BOOTLOADER_INFO_INDEX, 0).await.unwrap());
        // Config - application mode, can reset to bootloader
        assert_eq!(3, client.read_u32(BOOTLOADER_INFO_INDEX, 1).await.unwrap());
        // Number of sections
        assert_eq!(1, client.read_u8(BOOTLOADER_INFO_INDEX, 2).await.unwrap());

        assert!(!object_dict2::BOOTLOADER_INFO.reset_flag());

        client
            .write_u32(BOOTLOADER_INFO_INDEX, 3, BOOTLOADER_RESET_CMD)
            .await
            .unwrap();

        assert!(object_dict2::BOOTLOADER_INFO.reset_flag());
    };

    test_with_background_process(&mut [&mut node], &mut bus.new_sender(), test_task).await;
}

#[serial_test::serial]
#[tokio::test]
async fn test_program() {
    let (mut node, mut client, mut bus) = setup_single_node(
        &object_dict3::OD_TABLE,
        &object_dict3::NODE_MBOX,
        &object_dict3::NODE_STATE,
    );

    struct Callbacks {
        erase_flag: AtomicBool,
        data: Mutex<RefCell<Vec<u8>>>,
        finalize_flag: AtomicBool,
    }

    impl Callbacks {
        fn erase_flag(&self) -> bool {
            self.erase_flag.load(Ordering::Relaxed)
        }

        fn data(&self) -> Vec<u8> {
            self.data.lock().unwrap().borrow_mut().clone()
        }

        fn finalize_flag(&self) -> bool {
            self.finalize_flag.load(Ordering::Relaxed)
        }
    }

    impl BootloaderSectionCallbacks for Callbacks {
        fn erase(&self) -> bool {
            self.erase_flag.store(true, Ordering::Relaxed);
            true
        }

        /// Write a chunk of data
        ///
        /// Write will be called 1 or more times after an erase with a sequence of new data to write to
        /// the section
        fn write(&self, data: &[u8]) {
            let write_buffer = self.data.lock().unwrap();
            write_buffer.borrow_mut().extend_from_slice(data);
        }

        /// Finalize writing a section
        ///
        /// Will be called once after all data has been written to allow the storage driver to finalize
        /// writing the data and return any errors.
        ///
        /// Returns true on successful write
        fn finalize(&self) -> bool {
            self.finalize_flag.store(true, Ordering::Relaxed);
            true
        }
    }

    let callbacks: &Callbacks = Box::leak(Box::new(Callbacks {
        erase_flag: AtomicBool::new(false),
        data: Mutex::new(RefCell::new(Vec::new())),
        finalize_flag: AtomicBool::new(false),
    }));

    object_dict3::BOOTLOADER_SECTION0.register_callbacks(callbacks);

    let _logger = BusLogger::new(bus.new_receiver());

    let test_task = async move {
        // Mode register should indicate this section is programmable
        assert_eq!(
            client.read_u8(BOOTLOADER_SECTION0_INDEX, 1).await.unwrap(),
            1
        );
        // Check the name value
        assert_eq!(
            client
                .read_visible_string(BOOTLOADER_SECTION0_INDEX, 2)
                .await
                .unwrap(),
            "application"
        );
        // Send erase command
        client
            .write_u32(BOOTLOADER_SECTION0_INDEX, 3, BOOTLOADER_ERASE_CMD)
            .await
            .unwrap();

        let download_data = Vec::from_iter(0u8..128);
        // Send program
        client
            .block_download(BOOTLOADER_SECTION0_INDEX, 4, &download_data)
            .await
            .unwrap();

        assert!(callbacks.erase_flag());
        assert_eq!(download_data, callbacks.data());
        assert!(callbacks.finalize_flag())
    };

    test_with_background_process(&mut [&mut node], &mut bus.new_sender(), test_task).await;
}
