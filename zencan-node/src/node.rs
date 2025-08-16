//! Implements the core Node object
//!

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
    storage::StoreObjectsCallback,
};
use crate::{node_state::NodeStateAccess, sdo_server::SdoServer};

use defmt_or_log::{debug, info};

type StoreNodeConfigCallback = dyn Fn(&NodeId) + Sync;

#[derive(Default)]
struct Callbacks {
    store_node_config: Option<&'static StoreNodeConfigCallback>,
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

/// The first step to creating a node
///
/// In order to create a [`Node`], you first have to create one of these from the static data. This
/// allows the node creation process to be broken into a setup and an init step, while allowing the
/// compiler to ensure the first is not forgotten.
///
/// The proper sequence is:
///
/// 1) Create the InitializedOd. This will register all of the zencan provided object callbacks, so
///    that these objects are accessible via the OD.
/// 2) Initialize application default values -- this is the time to write things like software
///    versions, serial number, or to restore object values that were previously stored to flash.
/// 3) Create the Node object from the InitializedOd.
#[derive(Clone)]
#[allow(missing_debug_implementations)]
pub struct InitNode {
    node_id: NodeId,
    mbox: &'static NodeMbox,
    state: &'static dyn NodeStateAccess,
    od: &'static [ODEntry<'static>],
}

impl InitNode {
    pub fn new(
        node_id: NodeId,
        mbox: &'static NodeMbox,
        state: &'static dyn NodeStateAccess,
        od: &'static [ODEntry<'static>],
    ) -> Self {
        Self {
            node_id,
            mbox,
            state,
            od,
        }
    }

    /// Convert the InitNode into a ready-to-operate [`Node`]
    ///
    /// Before calling finalize, make sure you've loaded any application specific values to the
    /// object dictionary
    pub fn finalize(self) -> Node {
        Node::new(self)
    }
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
pub struct Node {
    node_id: NodeId,
    nmt_state: NmtState,
    sdo_server: SdoServer,
    lss_slave: LssSlave,
    message_count: u32,
    od: &'static [ODEntry<'static>],
    mbox: &'static NodeMbox,
    state: &'static dyn NodeStateAccess,
    reassigned_node_id: Option<NodeId>,
    callbacks: Callbacks,
    next_heartbeat_time_us: u64,
    heartbeat_period_ms: u16,
    heartbeat_toggle: bool,
    auto_start: bool,
    last_process_time_us: u64,
}

impl Node {
    /// Create an [`InitNode`], the first step in creating a Node.
    ///
    /// Creating the InitNode registers all of the library provided callbacks to objects, e.g. the
    /// PDO object handlers, making them functional. After init, the application should register any
    /// custom handlers of its own, and then initialize any objects it needs to, before calling
    /// [`InitNode::finalize`] to create the node.
    pub fn init(
        node_id: NodeId,
        mbox: &'static NodeMbox,
        state: &'static dyn NodeStateAccess,
        od: &'static [ODEntry<'static>],
    ) -> InitNode {
        InitNode::new(node_id, mbox, state, od)
    }

    /// Create a new Node
    ///
    /// # Arguments
    ///
    /// - `node_id`: The initial ID for the node. It may be assigned an ID at boot time by the
    ///   application, or it may be left as [NodeId::Unconfigured].
    /// - `mbox`: The NodeMbox object created by code generator
    /// - `state`: The NodeState object created by code generator
    /// - `od`: The Object Dictionary, created by code generator
    fn new(source: InitNode) -> Self {
        let InitNode {
            node_id,
            mbox,
            state,
            od,
        } = source;

        let message_count = 0;
        let sdo_server = SdoServer::new();
        let lss_slave = LssSlave::new(LssConfig {
            identity: read_identity(od).unwrap(),
            node_id,
            store_supported: false,
        });
        let nmt_state = NmtState::Bootup;
        let reassigned_node_id = None;

        let heartbeat_period_ms = read_heartbeat_period(od).expect("Heartbeat object must exist");
        let next_heartbeat_time_us = 0;
        let heartbeat_toggle = false;
        let auto_start = read_autostart(od).expect("auto start object must exist");
        let last_process_time_us = 0;
        Self {
            node_id,
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
            heartbeat_toggle,
            auto_start,
            callbacks: Callbacks::default(),
            last_process_time_us,
        }
    }

    /// Manually set the node ID. Changing the node id will cause an NMT comm reset to occur,
    /// resetting communication parameter defaults and triggering a bootup heartbeat message if the
    /// ID is valid. Setting the node ID to 255 will put the node into unconfigured mode.
    pub fn set_node_id(&mut self, node_id: NodeId) {
        self.reassigned_node_id = Some(node_id);
    }

    /// Register a callback to store node configuration data persistently
    pub fn register_store_node_config(&mut self, cb: &'static StoreNodeConfigCallback) {
        self.callbacks.store_node_config = Some(cb);
    }

    /// Register a callback to store object data persistently
    pub fn register_store_objects(&mut self, cb: &'static StoreObjectsCallback) {
        self.state.storage_context().store_callback.store(Some(cb));
    }

    /// Run periodic processing
    ///
    /// This should be called periodically by the application so that the node can update it's
    /// state, send periodic messages, process received messages, etc.
    ///
    /// It is sufficient to call this based on a timer, but the [NodeMbox] object also provides a
    /// notification callback, which can be used by an application to accelerate the call to process
    /// when an action is required
    ///
    /// # Arguments
    /// - `now_us`: A monotonic time in microseconds. This is used for measuring time and triggering
    ///   time-based actions such as heartbeat transmission or SDO timeout
    /// - `send_cb`: A callback function for transmitting can messages
    ///
    /// # Returns
    ///
    /// A boolean indicating if objects were updated. This will be true when an SDO download has
    /// been completed, or when one or more RPDOs have been received.
    pub fn process(&mut self, now_us: u64, send_cb: &mut dyn FnMut(CanMessage)) -> bool {
        let elapsed = (now_us - self.last_process_time_us) as u32;
        self.last_process_time_us = now_us;

        let mut update_flag = false;
        if let Some(new_node_id) = self.reassigned_node_id.take() {
            self.node_id = new_node_id;
            self.nmt_state = NmtState::Bootup;
        }

        if self.nmt_state == NmtState::Bootup {
            // Set state before calling boot_up, so the heartbeat state is correct
            self.nmt_state = NmtState::PreOperational;
            self.boot_up(send_cb);
        }

        // If auto start is set on boot, and we already have an ID, we make the first transition to
        // Operational automatically
        if self.auto_start && self.node_id.is_configured() {
            self.auto_start = false;
            self.nmt_state = NmtState::Operational;
        }

        // Process SDO server
        let (resp, updated_index) =
            self.sdo_server
                .process(self.mbox.sdo_receiver(), elapsed, self.od);
        if let Some(resp) = resp {
            send_cb(resp.to_can_message(self.sdo_tx_cob_id()));
        }
        if updated_index.is_some() {
            update_flag = true;
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
            send_cb(resp.to_can_message(LSS_RESP_ID));

            if let Some(event) = self.lss_slave.pending_event() {
                info!("LSS Slave Event: {:?}", event);
                match event {
                    crate::lss_slave::LssEvent::StoreConfiguration => {
                        if let Some(cb) = self.callbacks.store_node_config {
                            (cb)(&self.node_id)
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
            self.send_heartbeat(send_cb);
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
            let global_trigger = self.state.get_pdo_sync().toggle();

            for pdo in self.state.get_tpdos() {
                if !(pdo.valid()) {
                    continue;
                }
                let transmission_type = pdo.transmission_type();
                if transmission_type >= 254 {
                    if global_trigger && pdo.read_events() {
                        let mut data = [0u8; 8];
                        pdo.read_pdo_data(&mut data);
                        let msg = CanMessage::new(pdo.cob_id(), &data);
                        send_cb(msg);
                    }
                } else if sync && pdo.sync_update() {
                    let mut data = [0u8; 8];
                    pdo.read_pdo_data(&mut data);
                    let msg = CanMessage::new(pdo.cob_id(), &data);
                    send_cb(msg);
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

        update_flag
    }

    fn handle_nmt_command(&mut self, cmd: NmtCommandSpecifier) {
        let prev_state = self.nmt_state;

        match cmd {
            NmtCommandSpecifier::Start => self.nmt_state = NmtState::Operational,
            NmtCommandSpecifier::Stop => self.nmt_state = NmtState::Stopped,
            NmtCommandSpecifier::EnterPreOp => self.nmt_state = NmtState::PreOperational,
            NmtCommandSpecifier::ResetApp => {
                // if let Some(cb) = self.app_reset_callback.as_mut() {
                //     cb();
                // }
                self.nmt_state = NmtState::Bootup;
            }
            NmtCommandSpecifier::ResetComm => self.nmt_state = NmtState::Bootup,
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

    fn boot_up(&mut self, sender: &mut dyn FnMut(CanMessage)) {
        // Reset the LSS slave with the new ID
        self.lss_slave.update_config(LssConfig {
            identity: read_identity(self.od).unwrap(),
            node_id: self.node_id,
            store_supported: self.callbacks.store_node_config.is_some(),
        });

        if let NodeId::Configured(node_id) = self.node_id {
            info!("Booting node with ID {}", node_id.raw());
            self.mbox.set_sdo_cob_id(Some(self.sdo_rx_cob_id()));
            self.send_heartbeat(sender);
        }
    }

    fn send_heartbeat(&mut self, sender: &mut dyn FnMut(CanMessage)) {
        if let NodeId::Configured(node_id) = self.node_id {
            let heartbeat = Heartbeat {
                node: node_id.raw(),
                toggle: self.heartbeat_toggle,
                state: self.nmt_state,
            };
            self.heartbeat_toggle = !self.heartbeat_toggle;
            sender(heartbeat.into());
            self.next_heartbeat_time_us += (self.heartbeat_period_ms as u64) * 1000;
        }
    }
}
