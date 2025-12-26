//! Implements mailbox for receiving CAN messages
use defmt_or_log::warn;
use zencan_common::{
    messages::{CanId, CanMessage},
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
    rx_pdos: &'static [Pdo],
    tx_pdos: &'static [Pdo],
    /// ID used for transmitting SDO server responses
    sdo_tx_cob_id: AtomicCell<Option<CanId>>,
    /// ID used for receiving SDO server requests
    sdo_rx_cob_id: AtomicCell<Option<CanId>>,
    sdo_comms: SdoComms,
    nmt_mbox: AtomicCell<Option<CanMessage>>,
    lss_receiver: LssReceiver,
    sync_flag: AtomicCell<bool>,
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
        let sync_flag = AtomicCell::new(false);
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

    pub(crate) fn read_sync_flag(&self) -> bool {
        self.sync_flag.take()
    }

    /// Store a received CAN message
    pub fn store_message(&self, msg: CanMessage) -> Result<(), CanMessage> {
        let id = msg.id();
        if id == zencan_common::messages::NMT_CMD_ID {
            self.nmt_mbox.store(Some(msg));
            self.process_notify();
            return Ok(());
        }

        if id == zencan_common::messages::SYNC_ID {
            self.sync_flag.store(true);
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
                let mut data = [0u8; 8];
                data[0..msg.data().len()].copy_from_slice(msg.data());
                rpdo.buffered_value.store(Some(data));
                return Ok(());
            }
        }

        if let Some(cob_id) = self.sdo_rx_cob_id.load() {
            if id == cob_id {
                self.sdo_comms.handle_req(msg.data());
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
            if let Some(buf) = pdo.buffered_value.take() {
                return Some(CanMessage::new(pdo.cob_id(), &buf));
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
