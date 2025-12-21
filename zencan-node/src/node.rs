//! Implements the core Node object
//!

use core::{convert::Infallible, sync::atomic::Ordering};

use zencan_common::{
    constants::object_ids,
    lss::LssIdentity,
    messages::{
        CanId, CanMessage, Heartbeat, NmtCommandSpecifier, NmtState, ZencanMessage, LSS_RESP_ID,
    },
    NodeId,
};

use crate::{
    lss_slave::{LssConfig, LssSlave},
    node_mbox::NodeMbox,
    object_dict::{find_object, ODEntry},
};
use crate::{node_state::NodeStateAccess, sdo_server::SdoServer};

use defmt_or_log::{debug, info};

pub type StoreNodeConfigFn<'a> = dyn FnMut(NodeId) + 'a;
pub type StoreObjectsFn<'a> = dyn Fn(&mut dyn embedded_io::Read<Error = Infallible>, usize) + 'a;
pub type StateChangeFn<'a> = dyn FnMut(&'a [ODEntry<'a>]) + 'a;

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
    /// should also do whatever is appropraite to reset its state to it's reset condition.
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
        }
    }
}

fn read_identity(od: &[ODEntry]) -> Option<LssIdentity> {
    let obj = find_object(od, object_ids::IDENTITY)?;
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

fn read_heartbeat_period(od: &[ODEntry]) -> Option<u16> {
    let obj = find_object(od, object_ids::HEARTBEAT_PRODUCER_TIME)?;
    obj.read_u16(0).ok()
}

fn read_autostart(od: &[ODEntry]) -> Option<bool> {
    let obj = find_object(od, object_ids::AUTO_START)?;
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
pub struct Node<'a> {
    node_id: NodeId,
    nmt_state: NmtState,
    sdo_server: SdoServer<'a>,
    lss_slave: LssSlave,
    message_count: u32,
    od: &'a [ODEntry<'a>],
    mbox: &'a NodeMbox,
    state: &'a dyn NodeStateAccess,
    reassigned_node_id: Option<NodeId>,
    next_heartbeat_time_us: u64,
    heartbeat_period_ms: u16,
    auto_start: bool,
    last_process_time_us: u64,
    callbacks: Callbacks<'a>,
    transmit_flag: bool,
}

impl<'a> Node<'a> {
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
        callbacks: Callbacks<'a>,
        mbox: &'a NodeMbox,
        state: &'a dyn NodeStateAccess,
        od: &'a [ODEntry<'a>],
    ) -> Self {
        let message_count = 0;
        let sdo_server = SdoServer::new();
        let lss_slave = LssSlave::new(LssConfig {
            identity: read_identity(od).unwrap(),
            node_id,
            store_supported: false,
        });
        let nmt_state = NmtState::Bootup;
        let reassigned_node_id = None;

        // Storage command is supported if the application provides a callback
        if callbacks.store_objects.is_some() {
            state
                .storage_context()
                .store_supported
                .store(true, Ordering::Relaxed);
        }

        let heartbeat_period_ms = read_heartbeat_period(od).unwrap_or(0);
        let next_heartbeat_time_us = 0;
        let auto_start = read_autostart(od).unwrap_or(false);
        let last_process_time_us = 0;
        let transmit_flag = false;

        let mut node = Self {
            node_id,
            callbacks,
            nmt_state,
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
        };

        node.reset_app();
        node
    }

    /// Manually set the node ID. Changing the node id will cause an NMT comm reset to occur,
    /// resetting communication parameter defaults and triggering a bootup heartbeat message if the
    /// ID is valid. Setting the node ID to 255 will put the node into unconfigured mode.
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
    pub fn process(&mut self, now_us: u64) -> bool {
        let elapsed = (now_us - self.last_process_time_us) as u32;
        self.last_process_time_us = now_us;

        self.transmit_flag = false;

        let mut update_flag = false;
        if let Some(new_node_id) = self.reassigned_node_id.take() {
            self.node_id = new_node_id;
            self.nmt_state = NmtState::Bootup;
        }

        if self.nmt_state == NmtState::Bootup {
            // Set state before calling boot_up, so the heartbeat state is correct
            self.enter_preoperational();
            self.boot_up();
        }

        // If auto start is set on boot, and we already have an ID, we make the first transition to
        // Operational automatically
        if self.auto_start && self.node_id.is_configured() {
            // Clear flag so that we will not automatically enter operational again until reboot
            self.auto_start = false;
            self.enter_operational();
        }

        // Process SDO server
        let (resp, updated_index) =
            self.sdo_server
                .process(self.mbox.sdo_receiver(), elapsed, self.od);
        if let Some(resp) = resp {
            self.send_message(resp.to_can_message(self.sdo_tx_cob_id()));
        }
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
            if let Some(cb) = &mut self.callbacks.store_objects {
                crate::persist::serialize(self.od, *cb);
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
                        if let Some(cb) = &mut self.callbacks.store_node_config {
                            (cb)(self.node_id)
                        }
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

        if self.nmt_state == NmtState::Operational {
            // check if a sync has been received
            let sync = self.mbox.read_sync_flag();

            // Swap the active TPDO flag set. Returns true if any object flags were set since last
            // toggle. Tracking the global trigger is a performance boost, at least in the frequent
            // case when no events have been triggered. The goal is for `process` to be as fast as
            // possible when it has nothing to do, so it can be called frequently with little cost.
            let global_trigger = self.state.object_flag_sync().toggle();

            for pdo in self.state.get_tpdos() {
                if !(pdo.valid()) {
                    continue;
                }
                let transmission_type = pdo.transmission_type();
                if transmission_type >= 254 {
                    if global_trigger && pdo.read_events() {
                        pdo.send_pdo();
                        self.transmit_flag = true;
                    }
                } else if sync && pdo.sync_update() {
                    pdo.send_pdo();
                    self.transmit_flag = true;
                }
            }

            for pdo in self.state.get_tpdos() {
                pdo.clear_events();
            }

            for rpdo in self.state.get_rpdos() {
                if !rpdo.valid() {
                    continue;
                }
                if let Some(new_data) = rpdo.buffered_value.take() {
                    rpdo.store_pdo_data(&new_data);
                    update_flag = true;
                }
            }
        }

        if self.transmit_flag {
            self.mbox.transmit_notify();
        }

        update_flag
    }

    fn handle_nmt_command(&mut self, cmd: NmtCommandSpecifier) {
        let prev_state = self.nmt_state;

        match cmd {
            NmtCommandSpecifier::Start => self.enter_operational(),
            NmtCommandSpecifier::Stop => self.enter_stopped(),
            NmtCommandSpecifier::EnterPreOp => self.enter_preoperational(),
            NmtCommandSpecifier::ResetApp => self.reset_app(),
            NmtCommandSpecifier::ResetComm => self.reset_comm(),
        }

        debug!(
            "NMT state changed from {:?} to {:?}",
            prev_state, self.nmt_state
        );
    }

    /// Get the current Node ID
    pub fn node_id(&self) -> u8 {
        self.node_id.into()
    }

    /// Get the current NMT state of the node
    pub fn nmt_state(&self) -> NmtState {
        self.nmt_state
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
        self.nmt_state = NmtState::Operational;
        if let Some(cb) = &mut self.callbacks.enter_operational {
            (*cb)(self.od);
        }
    }

    fn enter_stopped(&mut self) {
        self.nmt_state = NmtState::Stopped;
        if let Some(cb) = &mut self.callbacks.enter_stopped {
            (*cb)(self.od);
        }
    }

    fn enter_preoperational(&mut self) {
        self.nmt_state = NmtState::PreOperational;
        if let Some(cb) = &mut self.callbacks.enter_preoperational {
            (*cb)(self.od);
        }
    }

    fn reset_app(&mut self) {
        // TODO: All objects should get reset to their defaults, but that isn't yet supported
        for pdo in self.state.get_rpdos().iter().chain(self.state.get_tpdos()) {
            pdo.init_defaults(self.node_id);
        }

        if let Some(reset_app_cb) = &mut self.callbacks.reset_app {
            (*reset_app_cb)(self.od);
        }
        self.nmt_state = NmtState::Bootup;
    }

    fn reset_comm(&mut self) {
        for pdo in self.state.get_rpdos().iter().chain(self.state.get_tpdos()) {
            pdo.init_defaults(self.node_id);
        }
        if let Some(reset_comms_cb) = &mut self.callbacks.reset_comms {
            (*reset_comms_cb)(self.od);
        }
        self.nmt_state = NmtState::Bootup;
    }

    fn boot_up(&mut self) {
        // Reset the LSS slave with the new ID
        self.lss_slave.update_config(LssConfig {
            identity: read_identity(self.od).unwrap(),
            node_id: self.node_id,
            store_supported: self.callbacks.store_node_config.is_some(),
        });

        if let NodeId::Configured(node_id) = self.node_id {
            info!("Booting node with ID {}", node_id.raw());
            self.mbox.set_sdo_cob_id(Some(self.sdo_rx_cob_id()));
            self.send_heartbeat();
        }
    }

    fn send_heartbeat(&mut self) {
        if let NodeId::Configured(node_id) = self.node_id {
            let heartbeat = Heartbeat {
                node: node_id.raw(),
                toggle: false,
                state: self.nmt_state,
            };
            self.send_message(heartbeat.into());
            self.next_heartbeat_time_us += (self.heartbeat_period_ms as u64) * 1000;
        }
    }
}
