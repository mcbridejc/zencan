use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

use integration_tests::sim_bus::SimBus;
use zencan_client::{RawAbortCode, SdoClient, SdoClientError};
use zencan_common::{sdo::AbortCode, NodeId};
use zencan_node::object_dict::SubObjectAccess;
use zencan_node::Node;

mod utils;
use utils::{test_with_background_process, BusLogger};

#[tokio::test]
#[serial_test::serial]
async fn test_sdo_read() {
    const SLAVE_NODE_ID: u8 = 1;

    let od = &integration_tests::object_dict1::OD_TABLE;
    let state = &integration_tests::object_dict1::NODE_STATE;
    let mbox = &integration_tests::object_dict1::NODE_MBOX;

    let mut node = Node::new(NodeId::new(SLAVE_NODE_ID).unwrap(), mbox, state, od);
    let mut bus = SimBus::new(vec![mbox]);
    let mut sender = bus.new_sender();

    test_with_background_process(&mut [&mut node], &mut sender, async move {
        let sender = bus.new_sender();
        let receiver = bus.new_receiver();
        let mut client = SdoClient::new_std(SLAVE_NODE_ID, sender, receiver);

        client
            .download(0x3000, 0, &[0xa, 0xb, 0xc, 0xd])
            .await
            .unwrap();
        let read = client.upload(0x3000, 0).await.unwrap();

        assert_eq!(vec![0xa, 0xb, 0xc, 0xd], read);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_block_download() {
    const SLAVE_NODE_ID: u8 = 1;

    let od = &integration_tests::object_dict1::OD_TABLE;
    let state = &integration_tests::object_dict1::NODE_STATE;
    let mbox = &integration_tests::object_dict1::NODE_MBOX;
    let mut node = Node::new(NodeId::new(SLAVE_NODE_ID).unwrap(), mbox, state, od);
    let mut bus = SimBus::new(vec![mbox]);
    let mut sender = bus.new_sender();
    let _bus_logger = BusLogger::new(bus.new_receiver());

    test_with_background_process(&mut [&mut node], &mut sender, async move {
        let sender = bus.new_sender();
        let receiver = bus.new_receiver();
        let mut client = SdoClient::new_std(SLAVE_NODE_ID, sender, receiver);

        let data = Vec::from_iter(0..128);
        client.block_download(0x3006, 0, &data).await.unwrap();

        assert_eq!(
            data,
            integration_tests::object_dict1::OBJECT3006.get_value()[0..data.len()]
        );

        // Now do a long one which will require multiple blocks
        let data = Vec::from_iter((0..1200).map(|i| i as u8));
        client.block_download(0x3006, 0, &data).await.unwrap();

        assert_eq!(
            data,
            integration_tests::object_dict1::OBJECT3006.get_value()
        );
    })
    .await;
}

#[derive(Debug)]
struct MockDomainData {
    buffer: Mutex<RefCell<Vec<u8>>>,
    write_pos: AtomicUsize,
    end_flag: AtomicBool,
}

impl MockDomainData {
    pub fn new(buffer: Vec<u8>) -> Self {
        Self {
            buffer: Mutex::new(RefCell::new(buffer)),
            write_pos: AtomicUsize::new(0),
            end_flag: AtomicBool::new(false),
        }
    }

    pub fn get_data(&self) -> Vec<u8> {
        let lock = self.buffer.lock().unwrap();
        let buffer = lock.borrow();
        buffer.clone()
    }
}

impl SubObjectAccess for MockDomainData {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let lock = self.buffer.lock().unwrap();
        let buffer = lock.borrow();
        if offset < buffer.len() {
            let read_len = buf.len().min(buffer.len() - offset);
            buf[..read_len].copy_from_slice(&buffer[offset..offset + read_len]);
            Ok(read_len)
        } else {
            Ok(0)
        }
    }

    fn read_size(&self) -> usize {
        let lock = self.buffer.lock().unwrap();
        let buffer = lock.borrow_mut();
        buffer.len()
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        let lock = self.buffer.lock().unwrap();
        let mut buffer = lock.borrow_mut();
        if data.len() > buffer.len() {
            return Err(AbortCode::DataTypeMismatchLengthHigh);
        }
        buffer[0..data.len()].copy_from_slice(data);
        Ok(())
    }

    fn begin_partial(&self) -> Result<(), AbortCode> {
        self.write_pos.store(0, Ordering::Relaxed);
        Ok(())
    }

    fn write_partial(&self, buf: &[u8]) -> Result<(), AbortCode> {
        let lock = self.buffer.lock().unwrap();
        let mut buffer = lock.borrow_mut();
        let write_pos = self.write_pos.load(Ordering::Relaxed);
        if write_pos + buf.len() > buffer.len() {
            return Err(AbortCode::DataTypeMismatchLengthHigh);
        }
        buffer[write_pos..write_pos + buf.len()].copy_from_slice(buf);
        self.write_pos
            .store(write_pos + buf.len(), Ordering::Relaxed);
        Ok(())
    }

    fn end_partial(&self) -> Result<(), AbortCode> {
        self.end_flag.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[tokio::test]
#[serial_test::serial]
async fn test_domain_access() {
    const SLAVE_NODE_ID: u8 = 1;

    let od = &integration_tests::object_dict1::OD_TABLE;
    let state = &integration_tests::object_dict1::NODE_STATE;
    let mbox = &integration_tests::object_dict1::NODE_MBOX;
    let mut node = Node::new(NodeId::new(SLAVE_NODE_ID).unwrap(), mbox, state, od);
    let mut bus = SimBus::new(vec![mbox]);
    let mut sender = bus.new_sender();
    let _bus_logger = BusLogger::new(bus.new_receiver());

    let domain: &MockDomainData = Box::leak(Box::new(MockDomainData::new(vec![0; 1200])));

    integration_tests::object_dict1::OBJECT3007
        .value
        .register_handler(domain);

    test_with_background_process(&mut [&mut node], &mut sender, async move {
        let sender = bus.new_sender();
        let receiver = bus.new_receiver();
        let mut client = SdoClient::new_std(SLAVE_NODE_ID, sender, receiver);

        // Create a long chunk of data
        let mut write_data = Vec::from_iter((0..1200).map(|i| i as u8));

        // Do a small write
        client
            .download(0x3007, 0, &[0xa, 0xb, 0xc, 0xd])
            .await
            .unwrap();
        assert_eq!([0xa, 0xb, 0xc, 0xd], domain.get_data()[0..4]);
        assert!(!domain.end_flag.load(Ordering::Relaxed));

        // Do a large write
        client.block_download(0x3007, 0, &write_data).await.unwrap();
        assert_eq!(write_data, domain.get_data());
        assert!(domain.end_flag.load(Ordering::Relaxed));

        // Do tooo large a write
        write_data.extend_from_slice(&[0]);
        let result = client.block_download(0x3007, 0, &write_data).await;
        assert_eq!(
            SdoClientError::ServerAbort {
                index: 0x3007,
                sub: 0,
                abort_code: RawAbortCode::Valid(AbortCode::DataTypeMismatchLengthHigh)
            },
            result.unwrap_err()
        );
    })
    .await;
}
