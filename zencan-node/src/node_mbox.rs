//! Implements mailbox for receiving CAN messages
use defmt_or_log::warn;
use zencan_common::{
    messages::{CanId, CanMessage, SyncObject},
    AtomicCell,
};

use crate::{
    lss_slave::LssReceiver, pdo::Pdo, priority_queue::PriorityQueue, sdo_server::SdoComms,
};

pub trait CanMessageQueue: Send + Sync {
    fn push(&self, msg: CanMessage) -> Result<(), CanMessage>;

    fn pop(&self) -> Option<CanMessage>;
}

impl<const N: usize> CanMessageQueue for PriorityQueue<N, CanMessage> {
    fn push(&self, msg: CanMessage) -> Result<(), CanMessage> {
        let prio = msg.id().raw();
        self.push(prio, msg)
    }

    fn pop(&self) -> Option<CanMessage> {
        self.pop()
    }
}

/// A data structure to be shared between a receiving thread (e.g. a CAN controller IRQ) and the
/// [`Node`](crate::Node) object.
///
/// Incoming messages should be passed to [NodeMbox::store_message].
#[allow(missing_debug_implementations)]
pub struct NodeMbox {
    rx_pdos: &'static [Pdo<'static>],
    tx_pdos: &'static [Pdo<'static>],
    /// ID used for transmitting SDO server responses
    sdo_tx_cob_id: AtomicCell<Option<CanId>>,
    /// ID used for receiving SDO server requests
    sdo_rx_cob_id: AtomicCell<Option<CanId>>,
    sdo_comms: SdoComms,
    nmt_mbox: AtomicCell<Option<CanMessage>>,
    lss_receiver: LssReceiver,
    sync_flag: AtomicCell<Option<SyncObject>>,
    process_notify_cb: AtomicCell<Option<&'static (dyn Fn() + Sync)>>,
    transmit_notify_cb: AtomicCell<Option<&'static (dyn Fn() + Sync)>>,
    tx_queue: &'static dyn CanMessageQueue,
}

impl NodeMbox {
    /// Create a new NodeMbox
    ///
    /// # Args
    ///
    /// - `rx_pdos`: A slice of Pdo objects for all of the receive PDOs
    pub const fn new(
        rx_pdos: &'static [Pdo],
        tx_pdos: &'static [Pdo],
        tx_queue: &'static dyn CanMessageQueue,
        sdo_buffer: &'static mut [u8],
    ) -> Self {
        let sdo_rx_cob_id = AtomicCell::new(None);
        let sdo_tx_cob_id = AtomicCell::new(None);
        let sdo_comms = SdoComms::new(sdo_buffer);
        let nmt_mbox = AtomicCell::new(None);
        let lss_receiver = LssReceiver::new();
        let sync_flag = AtomicCell::new(None);
        let process_notify_cb = AtomicCell::new(None);
        let transmit_notify_cb = AtomicCell::new(None);
        Self {
            rx_pdos,
            tx_pdos,
            sdo_rx_cob_id,
            sdo_tx_cob_id,
            sdo_comms,
            nmt_mbox,
            lss_receiver,
            sync_flag,
            process_notify_cb,
            transmit_notify_cb,
            tx_queue,
        }
    }

    /// Set a callback for notification when a message is received and requires processing.
    ///
    /// It must be static. Usually this will be a static fn, but in some circumstances, it may be
    /// desirable to use Box::leak to pass a heap allocated closure instead.
    pub fn set_process_notify_callback(&self, callback: &'static (dyn Fn() + Sync)) {
        self.process_notify_cb.store(Some(callback));
    }

    fn process_notify(&self) {
        if let Some(notify_cb) = self.process_notify_cb.load() {
            notify_cb();
        }
    }

    /// Set a callback for when new transmit messages are queued
    ///
    /// This will be called during process anytime new messages are ready to be queued
    pub fn set_transmit_notify_callback(&self, callback: &'static (dyn Fn() + Sync)) {
        self.transmit_notify_cb.store(Some(callback));
    }

    pub(crate) fn transmit_notify(&self) {
        if let Some(notify_cb) = self.transmit_notify_cb.load() {
            notify_cb();
        }
    }

    pub(crate) fn set_sdo_rx_cob_id(&self, cob_id: Option<CanId>) {
        self.sdo_rx_cob_id.store(cob_id);
    }

    pub(crate) fn set_sdo_tx_cob_id(&self, cob_id: Option<CanId>) {
        self.sdo_tx_cob_id.store(cob_id);
    }

    pub(crate) fn sdo_comms(&self) -> &SdoComms {
        &self.sdo_comms
    }

    pub(crate) fn read_nmt_mbox(&self) -> Option<CanMessage> {
        self.nmt_mbox.take()
    }

    pub(crate) fn lss_receiver(&self) -> &LssReceiver {
        &self.lss_receiver
    }

    pub(crate) fn read_sync_flag(&self) -> Option<SyncObject> {
        self.sync_flag.take()
    }

    /// Store a received CAN message
    ///
    /// If the message is recognized and handled, `Ok(())` is returned. Otherwise, the message is
    /// returned inside an Err.
    pub fn store_message(&self, msg: CanMessage) -> Result<(), CanMessage> {
        let id = msg.id();
        if id == zencan_common::messages::NMT_CMD_ID {
            self.nmt_mbox.store(Some(msg));
            self.process_notify();
            return Ok(());
        }

        if id == zencan_common::messages::SYNC_ID {
            let sync_object = SyncObject::from(msg);
            self.sync_flag.store(Some(sync_object));
            self.process_notify();
            return Ok(());
        }

        if id == zencan_common::messages::LSS_REQ_ID {
            if let Ok(lss_req) = msg.data().try_into() {
                if self.lss_receiver.handle_req(lss_req) {
                    self.process_notify();
                }
            } else {
                warn!("Invalid LSS request");
                return Err(msg);
            }
            return Ok(());
        }

        for rpdo in self.rx_pdos {
            if !rpdo.valid() {
                continue;
            }
            if id == rpdo.cob_id() {
                rpdo.buffered_value
                    .store(Some(CanMessage::new(rpdo.cob_id(), msg.data())));
                return Ok(());
            }
        }

        if let Some(cob_id) = self.sdo_rx_cob_id.load() {
            if id == cob_id {
                if self.sdo_comms.handle_req(msg.data()) {
                    self.process_notify();
                }
                return Ok(());
            }
        }

        Err(msg)
    }

    /// Get the next message ready for transmit
    ///
    /// Messages are prioritized as follows:
    ///
    /// - TPDOs first, if available, starting with TPDO0
    /// - Other non-SDO messages (SYNC, LSS, NMT)
    /// - SDO server responses    
    pub fn next_transmit_message(&self) -> Option<CanMessage> {
        for pdo in self.tx_pdos.iter() {
            if let Some(msg) = pdo.buffered_value.take() {
                return Some(msg);
            }
        }

        if let Some(msg) = self.tx_queue.pop() {
            return Some(msg);
        }

        if let Some(msg) = self.sdo_comms.next_transmit_message() {
            if let Some(id) = self.sdo_tx_cob_id.load() {
                return Some(CanMessage::new(id, &msg));
            }
        }

        None
    }

    /// Store a message for transmission in the general transmit queue
    pub fn queue_transmit_message(&self, msg: CanMessage) -> Result<(), CanMessage> {
        self.tx_queue.push(msg)
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use zencan_common::{
        messages::SDO_REQ_BASE,
        sdo::{BlockSegment, SdoRequest},
    };

    use crate::object_dict::ODEntry;

    use super::*;

    #[allow(unused)]
    struct TestObjects {
        od: &'static [ODEntry<'static>],
        rpdos: &'static [Pdo<'static>],
        tpdos: &'static [Pdo<'static>],
        txq: &'static dyn CanMessageQueue,
        mbox: NodeMbox,
    }

    const SDO_RX_COB_ID: CanId = CanId::std(SDO_REQ_BASE + 1);

    fn create_test_objects() -> TestObjects {
        let od = Box::leak(Box::new([]));
        let nmt_state = Box::leak(Box::new(AtomicCell::new(
            zencan_common::nmt::NmtState::Operational,
        )));
        let rpdos = Box::leak(Box::new([Pdo::new(od, nmt_state)]));
        let tpdos = Box::leak(Box::new([Pdo::new(od, nmt_state)]));
        let txq = Box::leak(Box::new(PriorityQueue::<4, CanMessage>::new()));
        let sdo_buffer = Box::leak(Box::new([0; 128]));
        let mbox = NodeMbox::new(rpdos, tpdos, txq, sdo_buffer);
        mbox.set_sdo_rx_cob_id(Some(SDO_RX_COB_ID));
        TestObjects {
            od,
            rpdos,
            tpdos,
            txq,
            mbox,
        }
    }

    /// When receiving unrecognized messages, it should return an error
    #[test]
    fn test_unrecognized_id_returns_error() {
        let obj = create_test_objects();
        assert!(obj
            .mbox
            .store_message(CanMessage::new(CanId::Std(0x123), &[]))
            .is_err());
    }

    #[test]
    /// Test response to SDO requests
    fn test_sdo_requests() {
        let obj = create_test_objects();

        // Setup a callback for process notification
        let process_flag = Box::leak(Box::new(Arc::new(AtomicBool::new(false))));
        let process_flag_cb = process_flag.clone();
        let process_cb = Box::leak(Box::new(move || {
            process_flag_cb.store(true, Ordering::Relaxed);
        }));
        obj.mbox.set_process_notify_callback(process_cb);

        // Initiate an upload, expect process notification
        let req = SdoRequest::initiate_upload(0, 0);
        assert!(obj
            .mbox
            .store_message(req.to_can_message(SDO_RX_COB_ID))
            .is_ok());
        assert!(process_flag.swap(false, Ordering::Relaxed));
        assert_eq!(Some(req), obj.mbox.sdo_comms().take_request());

        // When SDO server is in the middle of a block upload, message should not trigger a process
        // notify but the data should be stored in the sdo buffer
        obj.mbox.sdo_comms().begin_block_download(100);
        let req = BlockSegment {
            c: false,
            seqnum: 1,
            data: [1, 2, 3, 4, 5, 6, 7],
        };
        assert!(obj
            .mbox
            .store_message(req.to_can_message(SDO_RX_COB_ID))
            .is_ok());
        assert_eq!(false, process_flag.swap(false, Ordering::Relaxed));
        assert_eq!(None, obj.mbox.sdo_comms().take_request());
        let buf = obj.mbox.sdo_comms().borrow_buffer();
        assert_eq!([1, 2, 3, 4, 5, 6, 7], buf[0..7]);
    }
}
