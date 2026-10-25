//! Implements the core Node object
//!

use core::{convert::Infallible, marker::PhantomData, sync::atomic::Ordering};

use zencan_common::{
    can::{CanId, CanMessage},
    object_model::object_ids,
    protocol::{
        Heartbeat, LssIdentity, NmtCommandSpecifier, NmtState, SyncObject, ZencanMessage,
        LSS_RESP_ID,
    },
    AtomicCell, NodeId,
};

use crate::{
    lss_slave::{LssConfig, LssSlave},
    node_mbox::NodeMbox,
    node_state::NmtStateAccess as _,
    object_dict::{ODEntry, ObjectDictionary, ObjectLookup},
    pdo::{MappingEntry, N_MAPPING_PARAMS},
    sdo_server::SdoServer,
    NodeState,
};

use defmt_or_log::{debug, info};

pub type StoreNodeConfigFn<'a> = dyn FnMut(NodeId) + 'a;
pub type StoreObjectsFn<'a> = dyn Fn(&mut dyn embedded_io::Read<Error = Infallible>, usize) + 'a;
pub type StateChangeFn<'a> = dyn FnMut(&[ODEntry<'static>]) + 'a;
pub type SyncReceiveFn<'a> = dyn FnMut(SyncObject) + 'a;
pub type PdoReceiveFn<'a> = dyn for<'b> FnMut(u8, &'b [MappingEntry<'a>]);

/// Collection of callbacks events which Node object can call.
///
/// Most are optional, and may be implemented by the application or not.
#[allow(missing_debug_implementations)]
#[derive(Default)]
pub struct Callbacks<'a> {
    /// Store node config to flash
    ///
    /// An application should implement this callback in order to support storing a configured node
    /// ID persistently. It is triggered when the LSS StoreConfiguration command is received. The
    /// passed NodeId should be stored, and used when creating the [`Node`] object on the next boot.
    pub store_node_config: Option<&'a mut StoreNodeConfigFn<'a>>,

    /// Store object data to persistent flash
    ///
    /// The bytes read from the provided reader (arg 1) should be stored. The total number of bytes
    /// in the stream is given in the second arg.
    pub store_objects: Option<&'a mut StoreObjectsFn<'a>>,

    /// The RESET_APP NMT state has been entered
    ///
    /// If the application supported storing persistent object values, it should restore them now
    /// using the [`restore_stored_objects`](crate::restore_stored_objects) method. The application
    /// must reset its own state here as well, including values for any runtime-registered callback
    /// object handlers. The global default reset runs before this callback, so default values will
    /// be loaded into all storage objects.
    pub reset_app: Option<&'a mut StateChangeFn<'a>>,

    /// The RESET_COMMS NMT state has been entered
    ///
    /// During RESET COMMS, communications objects (i.e. 0x1000-0x1fff) are reset to their boot up
    /// values. Application which store persistent object values should restore ONLY THE COMM
    /// OBJECTS now, using the [`restore_stored_comm_objects`](crate::restore_stored_comm_objects)
    /// function.
    ///
    /// This event will only be triggered by an NMT RESET_COMMS command -- when a RESET_APP event
    /// occurs, only the reset_app callback is called.
    pub reset_comms: Option<&'a mut StateChangeFn<'a>>,

    /// The node is entering OPERATIONAL state
    pub enter_operational: Option<&'a mut StateChangeFn<'a>>,

    /// The node is entering the STOPPED state
    pub enter_stopped: Option<&'a mut StateChangeFn<'a>>,

    /// The node is entering the PRE-OPERATIONAL state
    pub enter_preoperational: Option<&'a mut StateChangeFn<'a>>,

    /// The node has received a SYNC object
    pub sync_received: Option<&'a mut SyncReceiveFn<'a>>,

    /// The node has received a PDO
    pub pdo_received: Option<&'a mut PdoReceiveFn<'a>>,
}

impl<'a> Callbacks<'a> {
    /// Create a new Callbacks struct with the provided send_message callback
    pub const fn new() -> Self {
        Self {
            store_node_config: None,
            store_objects: None,
            reset_app: None,
            reset_comms: None,
            enter_operational: None,
            enter_stopped: None,
            enter_preoperational: None,
            sync_received: None,
            pdo_received: None,
        }
    }
}

/// Events which a [`Node`] reports to the application
///
/// Every method has an empty default, so an implementation only overrides the events it handles.
/// [`Callbacks`] implements it from a set of optional closures, and `()` implements it with no
/// callbacks at all.
///
/// A [`Node`] is `Send` when its callbacks are, so it can be built in one context and run in
/// another, e.g. moved into an RTIC task. [`Callbacks`] holds plain closure references and is not
/// `Send`; implement this trait on a `Send` type, or use `()`, for that.
pub trait NodeCallbacks {
    /// Whether [`store_node_config`](Self::store_node_config) is implemented
    ///
    /// The node only accepts the LSS StoreConfiguration command when it is.
    fn store_node_config_supported(&self) -> bool {
        false
    }

    /// Store node config to flash; see [`Callbacks::store_node_config`]
    fn store_node_config(&mut self, _node_id: NodeId) {}

    /// Whether [`store_objects`](Self::store_objects) is implemented
    ///
    /// The node only accepts the object save command when it is.
    fn store_objects_supported(&self) -> bool {
        false
    }

    /// Store object data to persistent flash; see [`Callbacks::store_objects`]
    fn store_objects(
        &mut self,
        _reader: &mut dyn embedded_io::Read<Error = Infallible>,
        _len: usize,
    ) {
    }

    /// The RESET_APP NMT state has been entered; see [`Callbacks::reset_app`]
    fn reset_app(&mut self, _od: &[ODEntry<'static>]) {}

    /// The RESET_COMMS NMT state has been entered; see [`Callbacks::reset_comms`]
    fn reset_comms(&mut self, _od: &[ODEntry<'static>]) {}

    /// The node is entering OPERATIONAL state
    fn enter_operational(&mut self, _od: &[ODEntry<'static>]) {}

    /// The node is entering the STOPPED state
    fn enter_stopped(&mut self, _od: &[ODEntry<'static>]) {}

    /// The node is entering the PRE-OPERATIONAL state
    fn enter_preoperational(&mut self, _od: &[ODEntry<'static>]) {}

    /// The node has received a SYNC object
    fn sync_received(&mut self, _sync: SyncObject) {}

    /// The node has received a PDO
    fn pdo_received(&mut self, _pdo: u8, _mapping: &[MappingEntry<'static>]) {}
}

impl NodeCallbacks for () {}

impl NodeCallbacks for Callbacks<'_> {
    fn store_node_config_supported(&self) -> bool {
        self.store_node_config.is_some()
    }

    fn store_node_config(&mut self, node_id: NodeId) {
        if let Some(cb) = &mut self.store_node_config {
            cb(node_id)
        }
    }

    fn store_objects_supported(&self) -> bool {
        self.store_objects.is_some()
    }

    fn store_objects(
        &mut self,
        reader: &mut dyn embedded_io::Read<Error = Infallible>,
        len: usize,
    ) {
        if let Some(cb) = &mut self.store_objects {
            cb(reader, len)
        }
    }

    fn reset_app(&mut self, od: &[ODEntry<'static>]) {
        if let Some(cb) = &mut self.reset_app {
            cb(od)
        }
    }

    fn reset_comms(&mut self, od: &[ODEntry<'static>]) {
        if let Some(cb) = &mut self.reset_comms {
            cb(od)
        }
    }

    fn enter_operational(&mut self, od: &[ODEntry<'static>]) {
        if let Some(cb) = &mut self.enter_operational {
            cb(od)
        }
    }

    fn enter_stopped(&mut self, od: &[ODEntry<'static>]) {
        if let Some(cb) = &mut self.enter_stopped {
            cb(od)
        }
    }

    fn enter_preoperational(&mut self, od: &[ODEntry<'static>]) {
        if let Some(cb) = &mut self.enter_preoperational {
            cb(od)
        }
    }

    fn sync_received(&mut self, sync: SyncObject) {
        if let Some(cb) = &mut self.sync_received {
            cb(sync)
        }
    }

    fn pdo_received(&mut self, pdo: u8, mapping: &[MappingEntry<'static>]) {
        if let Some(cb) = &mut self.pdo_received {
            cb(pdo, mapping)
        }
    }
}

fn read_identity(od: &[ODEntry<'static>]) -> Option<LssIdentity> {
    let obj = od.find_object(object_ids::IDENTITY)?;
    let vendor_id = obj.read_u32(1).ok()?;
    let product_code = obj.read_u32(2).ok()?;
    let revision = obj.read_u32(3).ok()?;
    let serial = obj.read_u32(4).ok()?;
    Some(LssIdentity {
        vendor_id,
        product_code,
        revision,
        serial,
    })
}

fn read_heartbeat_period(od: &dyn ObjectLookup<'_>) -> Option<u16> {
    let obj = od.find_object(object_ids::HEARTBEAT_PRODUCER_TIME)?;
    obj.read_u16(0).ok()
}

fn read_autostart(od: &dyn ObjectLookup<'_>) -> Option<bool> {
    let obj = od.find_object(object_ids::AUTO_START)?;
    Some(obj.read_u8(0).unwrap() != 0)
}

/// The main object representing a node
///
/// # Operation
///
/// The node is run by polling the [`Node::process`] method in your application. It is safe to call
/// this method as frequently as you like. There is no hard minimum for call frequency, but calling
/// your node's responses to messages will be delayed until process is called, and this will slow
/// down communication to your node. It is recommended to register a callback using
/// [`NodeMbox::set_process_notify_callback`], and use this callback to trigger an immediate call to
/// process, e.g. by waking a task or signaling the processing thread.
#[allow(missing_debug_implementations)]
pub struct Node<'a, C = Callbacks<'a>> {
    node_id: NodeId,
    sdo_server: SdoServer<'static>,
    lss_slave: LssSlave,
    message_count: u32,
    od: &'static dyn ObjectDictionary<'static>,
    mbox: &'static NodeMbox,
    state: &'static NodeState<'static>,
    reassigned_node_id: Option<NodeId>,
    next_heartbeat_time_us: u64,
    heartbeat_period_ms: u16,
    auto_start: bool,
    last_process_time_us: u64,
    callbacks: C,
    transmit_flag: bool,
    _callbacks_lifetime: PhantomData<&'a ()>,
}

impl<C: NodeCallbacks> Node<'_, C> {
    /// Create a new [`Node`]
    ///
    /// # Arguments
    ///
    /// * `node_id` - Initial node ID assignment
    /// * `mbox` - The `NODE_MBOX` object created by `zencan-build`
    /// * `state` - The `NODE_STATE` state object created by `zencan-build`
    /// * `od` - The `OD_TABLE` object containing the object dictionary created by `zencan-build`
    pub fn new(
        node_id: NodeId,
        callbacks: C,
        mbox: &'static NodeMbox,
        state: &'static NodeState<'static>,
        od: &'static dyn ObjectDictionary,
    ) -> Self {
        let message_count = 0;
        let sdo_server = SdoServer::new();
        let lss_slave = LssSlave::new(LssConfig {
            identity: read_identity(od.od_table()).unwrap_or_default(),
            node_id,
            store_supported: false,
        });
        let reassigned_node_id = None;

        // Storage command is supported if the application provides a callback
        state
            .storage_context()
            .store_supported
            .store(callbacks.store_objects_supported(), Ordering::Relaxed);

        let heartbeat_period_ms = read_heartbeat_period(od).unwrap_or(0);
        let next_heartbeat_time_us = 0;
        let auto_start = read_autostart(od).unwrap_or(false);
        let last_process_time_us = 0;
        let transmit_flag = false;

        for pdo in state.rpdos().iter().chain(state.tpdos()) {
            pdo.set_node_id(node_id);
        }

        let mut node = Self {
            node_id,
            callbacks,
            sdo_server,
            lss_slave,
            message_count,
            od,
            mbox,
            state,
            reassigned_node_id,
            next_heartbeat_time_us,
            heartbeat_period_ms,
            auto_start,
            last_process_time_us,
            transmit_flag,
            _callbacks_lifetime: PhantomData,
        };

        node.reset_app();
        node
    }

    /// Manually set the node ID. Changing the node id will cause an NMT comm reset to occur,
    /// resetting communication parameter defaults and triggering a bootup heartbeat message if the
    /// ID is valid. Setting the node ID to 255 will put the node into unconfigured mode.
    /// The assignment and PDO IDs are updated together on the next process call.
    pub fn set_node_id(&mut self, node_id: NodeId) {
        self.reassigned_node_id = Some(node_id);
    }

    /// Run periodic processing
    ///
    /// This should be called periodically by the application so that the node can update it's
    /// state, send periodic messages, process received messages, etc.
    ///
    /// It is sufficient to call this based on a timer, but the [NodeMbox] object also provides a
    /// notification callback, which can be used by an application to accelerate the call to process
    /// when an action is required.
    ///
    /// # Arguments
    /// - `now_us`: A monotonic time in microseconds. This is used for measuring time and triggering
    ///   time-based actions such as heartbeat transmission or SDO timeout
    ///
    /// # Returns
    ///
    /// A boolean indicating if objects were updated. This will be true when an SDO download has
    /// been completed, or when one or more RPDOs have been received.
    // Node is generic over its callbacks, so process is compiled in the application crate, where
    // inlining it into the caller grows the binary.
    #[inline(never)]
    pub fn process(&mut self, now_us: u64) -> bool {
        let elapsed = (now_us - self.last_process_time_us) as u32;
        self.last_process_time_us = now_us;

        self.transmit_flag = false;

        let mut update_flag = false;
        if let Some(new_node_id) = self.reassigned_node_id.take() {
            self.node_id = new_node_id;
            for pdo in self.state.rpdos().iter().chain(self.state.tpdos()) {
                pdo.set_node_id(new_node_id);
            }
            self.reset_comm();
        }

        if self.nmt_state() == NmtState::Bootup {
            self.boot_up();
            self.enter_preoperational();
        }

        // If auto start is set on boot, and we already have an ID, we make the first transition to
        // Operational automatically
        if self.auto_start && self.node_id.is_configured() {
            // Clear flag so that we will not automatically enter operational again until reboot
            self.auto_start = false;
            self.enter_operational();
        }

        // Process SDO server
        let (message_sent, updated_index) =
            self.sdo_server
                .process(self.mbox.sdo_comms(), elapsed, self.od);

        self.transmit_flag |= message_sent;
        if updated_index.is_some() {
            update_flag = true;
        }

        // Read and clear the store command flag
        if self
            .state
            .storage_context()
            .store_flag
            .swap(false, Ordering::Relaxed)
        {
            // If the flag is set, and the user has provided a callback, call it
            if self.callbacks.store_objects_supported() {
                let callbacks = &mut self.callbacks;
                crate::persist::serialize(self.od.od_table(), &mut |reader, len| {
                    callbacks.store_objects(reader, len)
                });
            }
        }

        // Process NMT
        if let Some(msg) = self.mbox.read_nmt_mbox() {
            if let Ok(ZencanMessage::NmtCommand(cmd)) = msg.try_into() {
                self.message_count += 1;
                // We cannot respond to NMT commands if we do not have a valid node ID

                if let NodeId::Configured(node_id) = self.node_id {
                    if cmd.node == 0 || cmd.node == node_id.raw() {
                        debug!("Received NMT command: {:?}", cmd.cs);
                        self.handle_nmt_command(cmd.cs);
                    }
                }
            }
        }

        if let Ok(Some(resp)) = self.lss_slave.process(self.mbox.lss_receiver()) {
            self.send_message(resp.to_can_message(LSS_RESP_ID));

            if let Some(event) = self.lss_slave.pending_event() {
                info!("LSS Slave Event: {:?}", event);
                match event {
                    crate::lss_slave::LssEvent::StoreConfiguration => {
                        self.callbacks.store_node_config(self.node_id)
                    }
                    crate::lss_slave::LssEvent::ActivateBitTiming {
                        table: _,
                        index: _,
                        delay: _,
                    } => (),
                    crate::lss_slave::LssEvent::ConfigureNodeId { node_id } => {
                        self.set_node_id(node_id)
                    }
                }
            }
        }

        if self.heartbeat_period_ms != 0 && now_us >= self.next_heartbeat_time_us {
            self.send_heartbeat();
            // Perform catchup if we are behind, e.g. if we have not send a heartbeat in a long
            // time because we have not been configured
            if self.next_heartbeat_time_us < now_us {
                self.next_heartbeat_time_us = now_us;
            }
        }

        // check if a sync has been received
        let sync = self.mbox.read_sync_flag();

        if self.nmt_state() == NmtState::Operational {
            // TODO Process RPDO when sync received

            // Swap the active TPDO flag set. Returns true if any object flags were set since last
            // toggle. Tracking the global trigger is a performance boost, at least in the frequent
            // case when no events have been triggered. The goal is for `process` to be as fast as
            // possible when it has nothing to do, so it can be called frequently with little cost.
            let global_trigger = self.state.object_flag_sync().toggle();

            for pdo in self.state.tpdos() {
                if !(pdo.valid()) {
                    continue;
                }
                let transmission_type = pdo.transmission_type();
                if transmission_type >= 254 {
                    if global_trigger && pdo.read_events() {
                        pdo.send_pdo();
                        self.transmit_flag = true;
                    }
                } else if sync.is_some() && pdo.sync_update() {
                    pdo.send_pdo();
                    self.transmit_flag = true;
                }
            }

            for pdo in self.state.tpdos() {
                pdo.clear_events();
            }

            for (i, rpdo) in self.state.rpdos().iter().enumerate() {
                if !rpdo.valid() {
                    continue;
                }
                if let Some(new_data) = rpdo.data.buffered_value.take() {
                    rpdo.store_pdo_data(&new_data);
                    let mapping: heapless::Vec<MappingEntry<'static>, N_MAPPING_PARAMS> =
                        rpdo.data.mapping_params[..rpdo.data.valid_maps.load().into()]
                            .iter()
                            .map(AtomicCell::load)
                            .map(|entry| entry.try_get_valid_entry().unwrap())
                            .collect();
                    self.callbacks.pdo_received(i as u8, mapping.as_slice());
                    update_flag = true;
                }
            }
        }

        // Sync callback active when in operational or preop states. It is called after PDO
        // processing, so that any pending RPDOs which are transferred on SYNC are transferred
        // before the callback is run
        if matches!(
            self.nmt_state(),
            NmtState::Operational | NmtState::PreOperational
        ) {
            if let Some(obj) = sync {
                self.callbacks.sync_received(obj);
            }
        }

        if self.transmit_flag {
            self.mbox.transmit_notify();
        }

        update_flag
    }

    fn handle_nmt_command(&mut self, cmd: NmtCommandSpecifier) {
        let prev_state = self.nmt_state();

        match cmd {
            NmtCommandSpecifier::Start => self.enter_operational(),
            NmtCommandSpecifier::Stop => self.enter_stopped(),
            NmtCommandSpecifier::EnterPreOp => self.enter_preoperational(),
            NmtCommandSpecifier::ResetApp => self.reset_app(),
            NmtCommandSpecifier::ResetComm => self.reset_comm(),
        }

        debug!(
            "NMT state changed from {:?} to {:?}",
            prev_state,
            self.nmt_state()
        );
    }

    /// Get the current Node ID
    pub fn node_id(&self) -> u8 {
        self.node_id.into()
    }

    /// Get the current NMT state of the node
    pub fn nmt_state(&self) -> NmtState {
        self.state.nmt_state()
    }

    /// Get the number of received messages
    pub fn rx_message_count(&self) -> u32 {
        self.message_count
    }

    fn sdo_tx_cob_id(&self) -> CanId {
        let node_id: u8 = self.node_id.into();
        CanId::Std(0x580 + node_id as u16)
    }

    fn sdo_rx_cob_id(&self) -> CanId {
        let node_id: u8 = self.node_id.into();
        CanId::Std(0x600 + node_id as u16)
    }

    fn send_message(&mut self, msg: CanMessage) {
        self.transmit_flag = true;
        // TODO: return  the error, and then handle it everywhere
        self.mbox.queue_transmit_message(msg).ok();
    }

    fn enter_operational(&mut self) {
        self.state.set_nmt_state(NmtState::Operational);
        self.callbacks.enter_operational(self.od.od_table());
    }

    fn enter_stopped(&mut self) {
        self.state.set_nmt_state(NmtState::Stopped);
        self.callbacks.enter_stopped(self.od.od_table());
    }

    fn enter_preoperational(&mut self) {
        self.state.set_nmt_state(NmtState::PreOperational);
        self.callbacks.enter_preoperational(self.od.od_table());
    }

    fn reset_object_values(&self, scope: crate::ResetScope) {
        self.od.reset_objects(scope);
    }

    fn prepare_reset(&mut self) {
        self.state.set_nmt_state(NmtState::PreOperational);
        self.sdo_server = SdoServer::new();
        self.mbox.reset();
        self.state
            .storage_context()
            .store_flag
            .store(false, Ordering::Relaxed);
    }

    fn finish_reset(&mut self) {
        self.heartbeat_period_ms = read_heartbeat_period(self.od).unwrap_or(0);
        self.next_heartbeat_time_us = self.last_process_time_us;
        self.state.set_nmt_state(NmtState::Bootup);
    }

    fn reset_app(&mut self) {
        self.prepare_reset();
        self.reset_object_values(crate::ResetScope::Application);
        self.state.set_nmt_state(NmtState::Bootup);

        self.callbacks.reset_app(self.od.od_table());
        self.finish_reset();
    }

    fn reset_comm(&mut self) {
        self.prepare_reset();
        self.reset_object_values(crate::ResetScope::Communication);

        self.state.set_nmt_state(NmtState::Bootup);

        self.callbacks.reset_comms(self.od.od_table());
        self.finish_reset();
    }

    fn boot_up(&mut self) {
        // Reset the LSS slave with the new ID
        self.lss_slave.update_config(LssConfig {
            identity: read_identity(self.od.od_table()).unwrap_or_default(),
            node_id: self.node_id,
            store_supported: self.callbacks.store_node_config_supported(),
        });

        if let NodeId::Configured(node_id) = self.node_id {
            info!("Booting node with ID {}", node_id.raw());
            for pdo in self.state.rpdos().iter().chain(self.state.tpdos()) {
                pdo.set_node_id(node_id.into());
            }
            self.mbox.set_sdo_rx_cob_id(Some(self.sdo_rx_cob_id()));
            self.mbox.set_sdo_tx_cob_id(Some(self.sdo_tx_cob_id()));
            self.send_heartbeat();
        }
    }

    fn send_heartbeat(&mut self) {
        if let NodeId::Configured(node_id) = self.node_id {
            let heartbeat = Heartbeat {
                node: node_id.raw(),
                toggle: false,
                state: self.nmt_state(),
            };
            self.send_message(heartbeat.into());
            self.next_heartbeat_time_us += (self.heartbeat_period_ms as u64) * 1000;
        }
    }
}

// A node is built in one context and often run in another, so it must be `Send` when its
// callbacks are.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Node<'static, ()>>();
};

#[cfg(test)]
mod tests {
    use zencan_common::{
        can::CanMessage,
        object_model::{object_ids, ObjectCode},
        protocol::{Heartbeat, NmtCommand, NmtCommandSpecifier, NmtState, NodeId, ZencanMessage},
    };

    use crate::{
        object_dict::{
            ODEntry, ObjectAccess, ObjectDictionary, ObjectLookup, ProvidesSubObjects,
            ScalarFieldU16, ScalarFieldU8, SubInfo, SubObjectAccess,
        },
        priority_queue::PriorityQueue,
        Node, NodeCallbacks, NodeMbox, NodeState,
    };

    struct VarObject<T: SubObjectAccess> {
        value: T,
    }

    impl<T: SubObjectAccess> VarObject<T> {
        pub fn new(value: T) -> Self {
            Self { value }
        }
    }
    impl<T: SubObjectAccess> ProvidesSubObjects for VarObject<T> {
        fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
            match sub {
                0 => Some((SubInfo::new_u8(), &self.value)),
                _ => None,
            }
        }

        fn object_code(&self) -> ObjectCode {
            ObjectCode::Var
        }
    }

    struct TestOd {
        entries: &'static [ODEntry<'static>],
    }

    impl ObjectLookup<'static> for TestOd {
        fn find_object(&self, index: u16) -> Option<&'static dyn ObjectAccess> {
            self.entries.find_object(index)
        }
    }

    impl ObjectDictionary<'static> for TestOd {
        fn od_table(&self) -> &'static [ODEntry<'static>] {
            self.entries
        }
        fn reset_objects(&self, _scope: crate::ResetScope) {}
    }

    #[allow(dead_code)]
    struct TestNodeCtx<CB> {
        node: Node<'static, CB>,
        mbox: &'static NodeMbox,
        state: &'static NodeState<'static>,
        object_dict: &'static dyn ObjectDictionary<'static>,
    }

    fn new_test_node<CB>(
        node_id: NodeId,
        entries: Option<&'static [ODEntry]>,
        callbacks: CB,
    ) -> TestNodeCtx<CB>
    where
        CB: NodeCallbacks,
    {
        let tx_queue = Box::leak(Box::new(PriorityQueue::<4, CanMessage>::new()));
        let sdo_buffer = Box::leak(Box::new([0u8; 100]));
        let mbox = Box::leak(Box::new(NodeMbox::new(&[], &[], tx_queue, sdo_buffer)));
        let entries = entries.unwrap_or(Box::leak(Box::new([])));
        let object_dict = Box::leak(Box::new(TestOd { entries }));
        let state = Box::leak(Box::new(NodeState::new(&[], &[])));

        let node = Node::new(node_id, callbacks, mbox, state, object_dict);

        TestNodeCtx {
            node,
            mbox,
            state,
            object_dict,
        }
    }

    #[test]
    fn test_node_autostart_enabled() {
        let autostart_object = Box::leak(Box::new(VarObject::new(ScalarFieldU8::new(1))));
        let entries = Box::leak(Box::new([ODEntry {
            index: object_ids::AUTO_START,
            object: autostart_object,
        }]));

        let mut sys = new_test_node(NodeId::new(1).unwrap(), Some(entries), ());
        let node = &mut sys.node;
        let mbox = &mut sys.mbox;

        assert_eq!(NmtState::Bootup, node.nmt_state());
        node.process(0);
        assert_eq!(NmtState::Operational, node.nmt_state());

        // On NMT commanded reset, it should not autostart again
        mbox.store_message(
            NmtCommand {
                cs: NmtCommandSpecifier::ResetApp,
                node: 1,
            }
            .into(),
        )
        .unwrap();

        node.process(0);
        node.process(0);

        assert_eq!(NmtState::PreOperational, node.nmt_state());
    }

    #[test]
    fn test_node_autostart_disabled() {
        let autostart_object = Box::leak(Box::new(VarObject::new(ScalarFieldU8::new(0))));
        let entries = Box::leak(Box::new([ODEntry {
            index: object_ids::AUTO_START,
            object: autostart_object,
        }]));

        let mut sys = new_test_node(NodeId::new(1).unwrap(), Some(entries), ());
        let node = &mut sys.node;

        assert_eq!(NmtState::Bootup, node.nmt_state());
        node.process(0);
        assert_eq!(NmtState::PreOperational, node.nmt_state());
    }

    /// Records the events a node reports, as an application's own callbacks type would
    #[derive(Default)]
    struct EventRecorder {
        events: Vec<&'static str>,
    }

    impl NodeCallbacks for EventRecorder {
        fn reset_app(&mut self, _od: &[ODEntry<'static>]) {
            self.events.push("reset_app");
        }

        fn enter_operational(&mut self, _od: &[ODEntry<'static>]) {
            self.events.push("enter_operational");
        }

        fn enter_preoperational(&mut self, _od: &[ODEntry<'static>]) {
            self.events.push("enter_preoperational");
        }
    }

    #[test]
    fn test_node_calls_custom_callbacks() {
        // Enable autostart so node transitions immediately into operational
        let autostart_object = Box::leak(Box::new(VarObject::new(ScalarFieldU8::new(1))));
        let entries = Box::leak(Box::new([ODEntry {
            index: object_ids::AUTO_START,
            object: autostart_object,
        }]));
        let callbacks = EventRecorder::default();
        let mut sys = new_test_node(NodeId::new(1).unwrap(), Some(entries), callbacks);
        let node = &mut sys.node;
        let mbox = &mut sys.mbox;

        node.process(0);

        mbox.store_message(
            NmtCommand {
                cs: NmtCommandSpecifier::ResetApp,
                node: 1,
            }
            .into(),
        )
        .unwrap();
        node.process(0);
        node.process(0);

        assert_eq!(
            node.callbacks.events,
            [
                "reset_app",
                "enter_preoperational",
                "enter_operational",
                "reset_app",
                "enter_preoperational",
            ]
        );
    }

    #[test]
    fn test_nmt_boot_up_messages() {
        let heartbeat_object = Box::leak(Box::new(VarObject::new(ScalarFieldU16::new(1000))));
        let entries = Box::leak(Box::new([ODEntry {
            index: object_ids::HEARTBEAT_PRODUCER_TIME,
            object: heartbeat_object,
        }]));
        let node_id = NodeId::new(1).unwrap();
        let mut sys = new_test_node(node_id, Some(entries), ());
        let node = &mut sys.node;
        let mbox = &mut sys.mbox;

        node.process(0);
        node.process(node.heartbeat_period_ms as u64 * 1000 + 1);

        mbox.store_message(
            NmtCommand {
                cs: NmtCommandSpecifier::Start,
                node: node_id.raw(),
            }
            .into(),
        )
        .unwrap();
        node.process(2 * node.heartbeat_period_ms as u64 * 1000 + 2);

        let mut heartbeats = Vec::new();
        while let Some(msg) = mbox.next_transmit_message() {
            if let Ok(ZencanMessage::Heartbeat(heartbeat)) = msg.try_into() {
                heartbeats.push(heartbeat);
            }
        }

        assert_eq!(
            vec![
                Heartbeat {
                    node: node_id.into(),
                    state: NmtState::Bootup,
                    toggle: false,
                },
                Heartbeat {
                    node: node_id.into(),
                    state: NmtState::PreOperational,
                    toggle: false,
                },
                Heartbeat {
                    node: node_id.into(),
                    state: NmtState::Operational,
                    toggle: false,
                }
            ],
            heartbeats
        );
    }
}
