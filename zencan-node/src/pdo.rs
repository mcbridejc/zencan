//! Implementation of PDO configuration objects and PDO transmission
//!
//! ## PDO Default Configuration
//!
//! PDO default configuration can be controlled in the device config, so that PDOs may be mapped to
//! certain object and enabled by default in a device. This is done in the `[pdos]` section of the
//! config, which is defined by the
//! [`PdoDefaultConfig`](https://docs.rs/zencan-build/latest/zencan_build/device_config/struct.PdoDefaultConfig.html)
//! struct.
//!
//! The default PDO COB ID may be specified as an absolute value, or it may be offset by the node ID
//! at runtime.
//!
//! Example default PDO config:
//!
//! ```toml
//! [pdos]
//! num_rpdo = 4
//! num_tpdo = 4
//!
//! # Enable TPDO1 to send on 0x200 + NODE_ID
//! [pdos.tpdo.1]
//! enabled = true
//! cob_id = 0x200
//! add_node_id = true
//! transmission_type = 254
//! mappings = [
//!     { index=0x2000, sub=1, size=32 },
//! ]
//!
//! # Configure RPDO0 to receive on extended ID 0x5000
//! [pdos.rpdo.0]
//! enabled = true
//! extended = true
//! rtr_disabled = false
//! cob_id = 0x5000
//! add_node_id = false
//! transmission_type = 254
//! mappings = [
//!     { index = 0x2000, sub=2, size=32 },
//! ]
//! ```

use crate::{
    node_frame::{NodeFrame, PdoPayload},
    node_state::NmtStateAccess,
    object_dict::{
        ConstField, ObjectAccess, ObjectLookup, ProvidesSubObjects, SubInfo, SubObjectAccess,
    },
};
use zencan_common::{
    can::{CanId, CanMessage},
    object_model::{AccessType, DataType, ObjectCode, PdoMappable, PdoMapping},
    protocol::{AbortCode, NmtState, NodeId},
    AtomicCell,
};

/// Specifies the number of mapping parameters supported per PDO
///
/// This bounds the number of mapped objects, not the PDO length: the mapped objects may span up to
/// [`CanFrame::MAX_LEN`](zencan_common::can::CanFrame::MAX_LEN) bytes, i.e. 8 bytes in classic CAN
/// and 64 bytes in CAN FD. As only byte-aligned mappings are supported, 8 entries are always enough
/// to fill a classic frame, and 8 entries of up to 8 bytes fill a CAN FD frame. Every PDO stores
/// this many entries, so raising it costs RAM on every PDO.
pub const N_MAPPING_PARAMS: usize = 8;

#[allow(missing_debug_implementations)]
#[derive(Clone, Copy)]
/// A validated PDO object mapping passed to receive callbacks.
pub struct MappingEntry<'a> {
    /// A reference to the object which is mapped
    pub object: &'a dyn ObjectAccess,
    /// The index of the mapped object
    pub index: u16,
    /// The index of the sub object mapped
    pub sub: u8,
    /// The length of the mapping in bytes
    pub length: u8,
}

#[derive(Clone, Copy)]
/// Data structure for storing a PDO object mapping
pub(crate) struct StoredMappingEntry<'a> {
    /// A reference to the object which is mapped
    pub object: Option<&'a dyn ObjectAccess>,
    /// The index of the mapped object
    pub index: u16,
    /// The index of the sub object mapped
    pub sub: u8,
    /// The length of the mapping in bytes
    pub length: u8,
}

impl<'a> StoredMappingEntry<'a> {
    pub fn is_valid(&self) -> bool {
        self.object.is_some()
    }

    /// Return a MappingEntry if this StoredMappingEntry is configured
    pub fn try_get_valid_entry(&self) -> Option<MappingEntry<'a>> {
        if self.object.is_some() {
            Some(MappingEntry {
                object: self.object.unwrap(),
                index: self.index,
                sub: self.sub,
                length: self.length,
            })
        } else {
            None
        }
    }

    pub const fn empty() -> Self {
        Self {
            object: None,
            index: 0,
            sub: 0,
            length: 0,
        }
    }
}

#[allow(missing_debug_implementations)]
/// Initialization values for a PDO
#[derive(Copy, Clone)]
pub struct PdoDefaults<'a> {
    cob_id: u32,
    flags: u8,
    transmission_type: u8,
    mappings: &'a [u32],
}

impl Default for PdoDefaults<'_> {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[allow(missing_docs)]
impl<'a> PdoDefaults<'a> {
    const ADD_NODE_ID_FLAG: usize = 0;
    const VALID_FLAG: usize = 1;
    const RTR_DISABLED_FLAG: usize = 2;
    const IS_EXTENDED_FLAG: usize = 3;

    /// The PDO defaults used when no other defaults are configured
    pub const DEFAULT: PdoDefaults<'a> = Self {
        cob_id: 0,
        flags: 0,
        transmission_type: 0,
        mappings: &[],
    };

    /// Create a new PdoDefaults object
    pub const fn new(
        cob_id: u32,
        extended: bool,
        add_node_id: bool,
        valid: bool,
        rtr_disabled: bool,
        transmission_type: u8,
        mappings: &'static [u32],
    ) -> Self {
        // Store flags as a single field to save those precious few bytes
        let mut flags = 0u8;
        if valid {
            flags |= 1 << Self::VALID_FLAG;
        }
        if rtr_disabled {
            flags |= 1 << Self::RTR_DISABLED_FLAG;
        }
        if add_node_id {
            flags |= 1 << Self::ADD_NODE_ID_FLAG;
        }
        if extended {
            flags |= 1 << Self::IS_EXTENDED_FLAG;
        }

        Self {
            cob_id,
            flags,
            transmission_type,
            mappings,
        }
    }

    pub const fn valid(&self) -> bool {
        self.flags & (1 << Self::VALID_FLAG) != 0
    }

    pub const fn rtr_disabled(&self) -> bool {
        self.flags & (1 << Self::RTR_DISABLED_FLAG) != 0
    }

    pub const fn add_node_id(&self) -> bool {
        self.flags & (1 << Self::ADD_NODE_ID_FLAG) != 0
    }

    pub const fn extended(&self) -> bool {
        self.flags & (1 << Self::IS_EXTENDED_FLAG) != 0
    }

    pub const fn can_id(&self, node_id: u8) -> CanId {
        let id = if self.add_node_id() {
            self.cob_id + node_id as u32
        } else {
            self.cob_id
        };
        if self.extended() {
            CanId::Extended(id)
        } else {
            CanId::Std(id as u16)
        }
    }
}

/// Mutable PDO storage, constructed once and reset through its cells.
///
/// `F` is the CAN frame type the PDO is exchanged in, [`CanMessage`] by default.
#[allow(missing_debug_implementations)]
pub struct PdoData<'a, F: NodeFrame = CanMessage> {
    /// Current node ID assignment; Unconfigured after default reset.
    node_id: AtomicCell<NodeId>,
    /// The COB-ID used to send or receive this PDO
    cob_id: AtomicCell<Option<CanId>>,
    /// Indicates if the PDO is enabled
    valid: AtomicCell<bool>,
    /// If set, this PDO cannot be requested via RTR
    ///
    /// Always reported as set for CAN FD, which has no remote frames.
    rtr_disabled: AtomicCell<bool>,
    /// Transmission type field (subindex 0x2)
    /// Determines when the PDO is sent/received
    ///
    /// 0 (unused): PDO is sent on receipt of SYNC, but only if the event has been triggered
    /// 1 - 240: PDO is sent on receipt of every Nth SYNC message
    /// 254: PDO is sent asynchronously on application request
    transmission_type: AtomicCell<u8>,
    /// Tracks the number of sync signals since this was last sent or received
    sync_counter: AtomicCell<u8>,
    /// The last received payload for an RPDO, or the payload ready to transmit for a TPDO
    ///
    /// This is a [`PdoBuffer<8>`](crate::PdoBuffer) for classic CAN, and a
    /// [`PdoBuffer<64>`](crate::PdoBuffer) for CAN FD.
    pub buffered_value: AtomicCell<Option<F::PdoPayload>>,
    /// Indicates how many of the values in mapping_params are valid
    ///
    /// This represents sub0 for the mapping object
    pub valid_maps: AtomicCell<u8>,
    /// The mapping parameters
    ///
    /// These specify which objects are mapped into the PDO
    pub(crate) mapping_params: [AtomicCell<StoredMappingEntry<'a>>; N_MAPPING_PARAMS],
}

impl<'a> PdoData<'a> {
    /// Construct disabled classic CAN PDO storage with no mappings or buffered frame.
    ///
    /// Use [`PdoData::default`] to construct storage for another frame type.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<F: NodeFrame> Default for PdoData<'_, F> {
    /// Construct disabled PDO storage with no mappings or buffered frame.
    fn default() -> Self {
        F::pdo_data_default()
    }
}

impl<F: NodeFrame> PdoData<'_, F> {
    pub(crate) fn default_impl() -> Self {
        let cob_id = AtomicCell::new(None);
        let node_id = AtomicCell::new(NodeId::Unconfigured);
        let valid = AtomicCell::new(false);
        let rtr_disabled = AtomicCell::new(false);
        let transmission_type = AtomicCell::new(0);
        let sync_counter = AtomicCell::new(0);
        let buffered_value = AtomicCell::new(None);
        let valid_maps = AtomicCell::new(0);
        let mapping_params =
            [const { AtomicCell::new(StoredMappingEntry::empty()) }; N_MAPPING_PARAMS];
        Self {
            node_id,
            cob_id,
            valid,
            rtr_disabled,
            transmission_type,
            sync_counter,
            buffered_value,
            valid_maps,
            mapping_params,
        }
    }
}
/// Represents a single PDO state
///
/// `F` is the CAN frame type the PDO is exchanged in, [`CanMessage`] by default.
#[allow(missing_debug_implementations)]
pub struct Pdo<'a, F: NodeFrame = CanMessage> {
    /// The object dictionary
    ///
    /// PDOs have to access other objects and use this to do so
    pub od: &'a dyn ObjectLookup<'a>,
    /// Accessor for the node NMT state
    pub nmt_state: &'a dyn NmtStateAccess,
    /// The allocated storage for associated data
    pub data: &'a PdoData<'a, F>,
    /// The default data for the PDO
    pub defaults: &'a PdoDefaults<'a>,
}

impl<'a, F: NodeFrame> Pdo<'a, F> {
    /// Create a new PDO object
    pub const fn new(
        od: &'a dyn ObjectLookup<'a>,
        nmt_state: &'a dyn NmtStateAccess,
        data: &'a PdoData<'a, F>,
        defaults: &'a PdoDefaults<'a>,
    ) -> Self {
        Self {
            od,
            nmt_state,
            data,
            defaults,
        }
    }

    /// Update the node ID assignment, preserving configured PDO parameters.
    pub(crate) fn set_node_id(&self, node_id: NodeId) {
        self.data.node_id.store(node_id);
    }

    /// Set the valid bit
    pub fn set_valid(&self, value: bool) {
        self.data.valid.store(value);
    }

    /// Get the valid bit value
    pub fn valid(&self) -> bool {
        self.data.valid.load()
    }

    /// Set the transmission type for this PDO
    pub fn set_transmission_type(&self, value: u8) {
        self.data.transmission_type.store(value);
    }

    /// Get the transmission type for this PDO
    pub fn transmission_type(&self) -> u8 {
        self.data.transmission_type.load()
    }

    /// Get the COB ID used for transmission of this PDO
    pub fn cob_id(&self) -> CanId {
        critical_section::with(|cs| self.cob_id_in(cs))
    }

    fn cob_id_in(&self, cs: critical_section::CriticalSection<'_>) -> CanId {
        self.data.cob_id.borrow(cs).get().unwrap_or_else(|| {
            if self.defaults.valid() {
                let id = match self.data.node_id.borrow(cs).get() {
                    NodeId::Unconfigured => 0,
                    NodeId::Configured(id) => id.raw(),
                };
                self.defaults.can_id(id)
            } else {
                CanId::std(0)
            }
        })
    }

    pub(crate) fn receive_frame(&self, msg: &F) -> bool {
        critical_section::with(|cs| {
            if self.data.valid.borrow(cs).get() && self.cob_id_in(cs) == msg.id() {
                self.data
                    .buffered_value
                    .borrow(cs)
                    .set(Some(F::PdoPayload::from_slice(msg.data())));
                true
            } else {
                false
            }
        })
    }

    pub(crate) fn take_frame(&self) -> Option<F> {
        critical_section::with(|cs| {
            let data = self.data.buffered_value.borrow(cs).take()?;
            // The payload never exceeds the frame's capacity, so this cannot fail
            F::try_new(self.cob_id_in(cs), &data).ok()
        })
    }

    /// Get the default COB ID for transmission of this PDO
    pub fn default_cob_id(&self) -> CanId {
        if !self.defaults.valid() {
            return CanId::std(0);
        }
        let defaults = self.defaults;
        let node_id = match self.data.node_id.load() {
            NodeId::Unconfigured => 0,
            NodeId::Configured(node_id) => node_id.raw(),
        };
        defaults.can_id(node_id)
    }

    /// This function should be called when a SYNC event occurs
    ///
    /// It will return true if the PDO should be sent in response to the SYNC event
    pub fn sync_update(&self) -> bool {
        if !self.data.valid.load() {
            return false;
        }

        let transmission_type = self.data.transmission_type.load();
        if transmission_type == 0 {
            // TODO: Figure out how to determine application "event" which triggers the PDO
            // For now, send every sync
            true
        } else if transmission_type <= 240 {
            // Atomically update this PDO's sync counter. If it has
            // reached the transmit threshold ("transmission_type"),
            // then reset it to zero.
            let r = self.data.sync_counter.fetch_update(|old| {
                let new = old + 1;
                if new >= transmission_type {
                    Some(0)
                } else {
                    Some(new)
                }
            });

            // We don't care if the update worked or not (because there's
            // nothing we can do about it). Just use whatever the old
            // value was.
            let cnt = match r {
                Ok(old) => old,
                Err(old) => old,
            } + 1;

            cnt == transmission_type
        } else {
            false
        }
    }

    /// Check mapped objects for TPDO event flag
    pub fn read_events(&self) -> bool {
        if !self.data.valid.load() {
            return false;
        }

        for i in 0..self.data.mapping_params.len() {
            let param = self.data.mapping_params[i].load();
            if !param.is_valid() {
                break;
            }
            if param.object.unwrap().read_event_flag(param.sub) {
                return true;
            }
        }
        false
    }

    fn nmt_state(&self) -> NmtState {
        self.nmt_state.nmt_state()
    }

    pub(crate) fn clear_events(&self) {
        for i in 0..self.data.mapping_params.len() {
            let param = self.data.mapping_params[i].load();
            if let Some(map_entry) = param.try_get_valid_entry() {
                map_entry.object.clear_events();
            }
        }
    }

    pub(crate) fn store_pdo_data(&self, data: &[u8]) {
        let mut offset = 0;
        let valid_maps = self.data.valid_maps.load() as usize;
        for (i, param) in self.data.mapping_params.iter().enumerate() {
            if i >= valid_maps {
                break;
            }
            if let Some(param) = param.load().try_get_valid_entry() {
                let length = param.length as usize;
                if offset + length > data.len() {
                    break;
                }
                let data_to_write = &data[offset..offset + length];
                // validity of the mappings must be validated during write, so that error here is not
                // possible
                param.object.write(param.sub, data_to_write).ok();
                offset += length;
            } else {
                // The first N params will be valid. If a None is reached, all remaining will be
                // None
                break;
            }
        }
    }

    pub(crate) fn send_pdo(&self) {
        // The mapped objects are read straight into the payload buffer, which has room for one
        // full frame of the PDO's frame type
        let mut payload = F::PdoPayload::default();
        let valid_maps = self.data.valid_maps.load() as usize;
        for (i, param) in self.data.mapping_params.iter().enumerate() {
            if i >= valid_maps {
                break;
            }
            if let Some(param) = param.load().try_get_valid_entry() {
                // Mappings beyond the frame capacity are not sent
                let Some(dest) = payload.extend_zeroed(param.length as usize) else {
                    break;
                };
                // validity of the mappings must be validated during write, so that error here is not
                // possible
                param.object.read(param.sub, 0, dest).ok();
            } else {
                // The first N params will be valid. If a None is reached, all remaining will be
                // None
                break;
            }
        }
        // If there is an old value here which has not been sent yet, replace it with the latest
        // Data will be sent by mbox in message handling thread.
        self.data.buffered_value.store(Some(payload));
    }

    /// Lookup a PDO mapped object and create a StoredMappingEntry if it is valid
    ///
    /// The returned StoredMappingEntry can be stored in the Pdo mappings and includes
    /// a reference to the mapped object for faster access when
    /// sending/receiving PDOs.
    ///
    /// This function may fail if the mapped object doesn't exist, or if it is
    /// too short.
    fn try_create_mapping_entry(
        &'a self,
        mapping: PdoMapping,
    ) -> Result<StoredMappingEntry<'a>, AbortCode> {
        let PdoMapping {
            index,
            sub,
            size: length,
        } = mapping;
        // length is in bits.
        if length == 0 || (length % 8) != 0 {
            // only support byte level access for now
            return Err(AbortCode::IncompatibleParameter);
        }
        let object = self.od.find_object(index).ok_or(AbortCode::NoSuchObject)?;
        let sub_info = object.sub_info(sub)?;
        if sub_info.size() < length as usize / 8 {
            return Err(AbortCode::IncompatibleParameter);
        }
        Ok(StoredMappingEntry {
            object: Some(object),
            index,
            sub,
            length: length / 8,
        })
    }

    /// Restore PDO defaults and clear the node ID to Unconfigured.
    pub fn init_defaults(&'a self) {
        F::pdo_init_defaults(self)
    }

    pub(crate) fn init_defaults_impl(&'a self) {
        let mut mappings = [StoredMappingEntry::empty(); N_MAPPING_PARAMS];
        let mut bytes = 0usize;
        let mut valid = self.defaults.mappings.len() <= N_MAPPING_PARAMS;
        for (slot, raw) in mappings.iter_mut().zip(self.defaults.mappings) {
            match self.try_create_mapping_entry(PdoMapping::from_object_value(*raw)) {
                Ok(entry) => {
                    bytes += entry.length as usize;
                    *slot = entry;
                }
                Err(_) => valid = false,
            }
        }
        valid &= bytes <= F::MAX_LEN;
        // Publish the complete configuration together; never retain mappings or
        // buffered data from the preceding configuration. Borrowing each cell
        // under this token avoids entering another critical section per field.
        critical_section::with(|cs| {
            self.data.valid.borrow(cs).set(false);
            self.data.cob_id.borrow(cs).set(None);
            self.data
                .rtr_disabled
                .borrow(cs)
                .set(self.defaults.rtr_disabled() || !F::SUPPORTS_RTR);
            self.data
                .transmission_type
                .borrow(cs)
                .set(self.defaults.transmission_type);
            self.data.sync_counter.borrow(cs).set(0);
            self.data.buffered_value.borrow(cs).set(None);
            for (cell, entry) in self.data.mapping_params.iter().zip(mappings) {
                cell.borrow(cs).set(if valid {
                    entry
                } else {
                    StoredMappingEntry::empty()
                });
            }
            self.data.valid_maps.borrow(cs).set(if valid {
                self.defaults.mappings.len() as u8
            } else {
                0
            });
            self.data
                .valid
                .borrow(cs)
                .set(valid && self.defaults.valid());
        });
    }
}

struct PdoCobSubObject<'a, F: NodeFrame> {
    pdo: &'a Pdo<'a, F>,
}

impl<'a, F: NodeFrame> PdoCobSubObject<'a, F> {
    pub const fn new(pdo: &'a Pdo<'a, F>) -> Self {
        Self { pdo }
    }

    /// Should the COB sub object be persisted
    ///
    /// The object is only persisted when a non-default COB ID has been assigned.
    pub fn should_persist(&self) -> bool {
        self.pdo.data.cob_id.load().is_some()
    }
}

impl<F: NodeFrame> SubObjectAccess for PdoCobSubObject<'_, F> {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let value = critical_section::with(|cs| {
            let cob_id = self.pdo.cob_id_in(cs);
            let mut value = cob_id.raw();
            if cob_id.is_extended() {
                value |= 1 << 29;
            }
            if self.pdo.data.rtr_disabled.borrow(cs).get() {
                value |= 1 << 30;
            }
            if !self.pdo.data.valid.borrow(cs).get() {
                value |= 1 << 31;
            }
            value
        });

        let bytes = value.to_le_bytes();
        if offset < bytes.len() {
            let read_len = buf.len().min(bytes.len() - offset);
            buf[0..read_len].copy_from_slice(&bytes[offset..offset + read_len]);
            Ok(read_len)
        } else {
            Ok(0)
        }
    }

    fn read_size(&self) -> usize {
        4
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        // Changing PDO config is only allowed during PreOperational state, or Bootup when the
        // defaults are loaded (Bootup is a always a short-lived state).
        let nmt_state = self.pdo.nmt_state();
        if nmt_state != NmtState::PreOperational && nmt_state != NmtState::Bootup {
            return Err(AbortCode::GeneralError);
        }
        if data.len() < 4 {
            Err(AbortCode::DataTypeMismatchLengthLow)
        } else if data.len() > 4 {
            Err(AbortCode::DataTypeMismatchLengthHigh)
        } else {
            let value = u32::from_le_bytes(data.try_into().unwrap());
            let not_valid = (value & (1 << 31)) != 0;
            // CAN FD has no remote frames, so RTR stays disabled whatever is written
            let no_rtr = (value & (1 << 30)) != 0 || !F::SUPPORTS_RTR;
            let extended_id = (value & (1 << 29)) != 0;

            let can_id = if extended_id {
                CanId::Extended(value & 0x1FFFFFFF)
            } else {
                CanId::Std((value & 0x7FF) as u16)
            };
            critical_section::with(|cs| {
                self.pdo.data.cob_id.borrow(cs).set(Some(can_id));
                self.pdo.data.rtr_disabled.borrow(cs).set(no_rtr);
                self.pdo.data.valid.borrow(cs).set(!not_valid);
            });
            Ok(())
        }
    }
}

struct PdoTransmissionTypeSubObject<'a, F: NodeFrame> {
    pdo: &'a Pdo<'a, F>,
}

impl<'a, F: NodeFrame> PdoTransmissionTypeSubObject<'a, F> {
    pub const fn new(pdo: &'a Pdo<'a, F>) -> Self {
        Self { pdo }
    }
}

impl<F: NodeFrame> SubObjectAccess for PdoTransmissionTypeSubObject<'_, F> {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        if offset > 1 {
            return Ok(0);
        }
        buf[0] = self.pdo.transmission_type();
        Ok(1)
    }

    fn read_size(&self) -> usize {
        1
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        // Changing PDO config is only allowed during PreOperational state, or Bootup when the
        // defaults are loaded (Bootup is a always a short-lived state).
        let nmt_state = self.pdo.nmt_state();
        if nmt_state != NmtState::PreOperational && nmt_state != NmtState::Bootup {
            return Err(AbortCode::GeneralError);
        }
        if data.is_empty() {
            Err(AbortCode::DataTypeMismatchLengthLow)
        } else {
            self.pdo.set_transmission_type(data[0]);
            Ok(())
        }
    }
}

/// Implements a PDO communications config object for both RPDOs and TPDOs
#[allow(missing_debug_implementations)]
pub struct PdoCommObject<'a, F: NodeFrame = CanMessage> {
    cob: PdoCobSubObject<'a, F>,
    transmission_type: PdoTransmissionTypeSubObject<'a, F>,
}

impl<'a, F: NodeFrame> PdoCommObject<'a, F> {
    /// Create a new PdoCommObject
    pub const fn new(pdo: &'a Pdo<'a, F>) -> Self {
        let cob = PdoCobSubObject::new(pdo);
        let transmission_type = PdoTransmissionTypeSubObject::new(pdo);
        Self {
            cob,
            transmission_type,
        }
    }
}

impl<F: NodeFrame> ProvidesSubObjects for PdoCommObject<'_, F> {
    fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
        match sub {
            0 => Some((
                SubInfo::MAX_SUB_NUMBER,
                const { &ConstField::new(2u8.to_le_bytes()) },
            )),
            1 => Some((
                SubInfo::new_u32()
                    .rw_access()
                    .persist(self.cob.should_persist()),
                &self.cob,
            )),
            2 => Some((
                SubInfo::new_u8().rw_access().persist(true),
                &self.transmission_type,
            )),
            _ => None,
        }
    }

    fn object_code(&self) -> ObjectCode {
        ObjectCode::Record
    }
}

/// Implements a PDO mapping config object for both TPDOs and RPDOs
#[allow(missing_debug_implementations)]
pub struct PdoMappingObject<'a, F: NodeFrame = CanMessage> {
    pdo: &'a Pdo<'a, F>,
}

impl<'a, F: NodeFrame> PdoMappingObject<'a, F> {
    /// Create a new PdoMappingObject
    pub const fn new(pdo: &'a Pdo<'a, F>) -> Self {
        Self { pdo }
    }
}

impl<F: NodeFrame> ObjectAccess for PdoMappingObject<'_, F> {
    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        if sub == 0 {
            if offset < 1 && !buf.is_empty() {
                buf[0] = self.pdo.data.valid_maps.load();
                Ok(1)
            } else {
                Ok(0)
            }
        } else if sub <= self.pdo.data.mapping_params.len() as u8 {
            let value = if let Some(param) = self.pdo.data.mapping_params[(sub - 1) as usize]
                .load()
                .try_get_valid_entry()
            {
                ((param.index as u32) << 16) + ((param.sub as u32) << 8) + param.length as u32 * 8
            } else {
                0u32
            };
            let bytes = value.to_le_bytes();
            let read_len = buf.len().min(bytes.len() - offset);
            buf[..read_len].copy_from_slice(&bytes[offset..offset + read_len]);
            Ok(read_len)
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn read_size(&self, sub: u8) -> Result<usize, AbortCode> {
        if sub == 0 {
            Ok(1)
        } else if sub <= N_MAPPING_PARAMS as u8 {
            Ok(4)
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn write(&self, sub: u8, data: &[u8]) -> Result<(), AbortCode> {
        // Changing PDO config is only allowed during PreOperational state, or Bootup when the
        // defaults are loaded (Bootup is a always a short-lived state).
        let nmt_state = self.pdo.nmt_state();
        if nmt_state != NmtState::PreOperational && nmt_state != NmtState::Bootup {
            return Err(AbortCode::GeneralError);
        }
        if sub == 0 {
            self.pdo.data.valid_maps.store(data[0]);
            Ok(())
        } else if sub <= self.pdo.data.mapping_params.len() as u8 {
            if data.len() != 4 {
                return Err(AbortCode::DataTypeMismatch);
            }
            let value = u32::from_le_bytes(data.try_into().unwrap());

            let mapping = PdoMapping::from_object_value(value);

            self.pdo.data.mapping_params[(sub - 1) as usize]
                .store(self.pdo.try_create_mapping_entry(mapping)?);
            Ok(())
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn object_code(&self) -> ObjectCode {
        ObjectCode::Record
    }

    fn sub_info(&self, sub: u8) -> Result<SubInfo, AbortCode> {
        if sub == 0 {
            Ok(SubInfo {
                data_type: DataType::UInt8,
                access_type: AccessType::Rw,
                pdo_mapping: PdoMappable::None,
                persist: true,
            })
        } else if sub <= self.pdo.data.mapping_params.len() as u8 {
            Ok(SubInfo {
                data_type: DataType::UInt32,
                access_type: AccessType::Rw,
                pdo_mapping: PdoMappable::None,
                persist: true,
            })
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_dict::{ScalarFieldU16, ScalarFieldU32, ScalarFieldU64};
    use zencan_common::can::{CanFdMessage, CanFrame};

    #[derive(Default)]
    struct TestObject {
        value: ScalarFieldU32,
    }

    struct TestOd<'a> {
        object1000: &'a TestObject,
    }

    impl<'a> ObjectLookup<'a> for TestOd<'a> {
        fn find_object(&self, index: u16) -> Option<&'a dyn ObjectAccess> {
            match index {
                0x1000 => Some(self.object1000),
                _ => None,
            }
        }
    }

    impl ProvidesSubObjects for TestObject {
        fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
            match sub {
                0 => Some((SubInfo::new_u32(), &self.value)),
                _ => None,
            }
        }

        fn object_code(&self) -> ObjectCode {
            ObjectCode::Var
        }
    }

    #[test]
    /// Assert that attempts to update PDO comms or mapping parameters fail when in operational mode
    pub fn test_changes_denied_while_operational() {
        let object1000 = TestObject::default();
        let od = TestOd {
            object1000: &object1000,
        };
        let nmt_state = AtomicCell::new(NmtState::PreOperational);
        let pdo_data = PdoData::new();
        let pdo_defaults = PdoDefaults::default();

        let pdo = Pdo::new(&od, &nmt_state, &pdo_data, &pdo_defaults);

        let comm_obj = PdoCommObject::new(&pdo);
        let mapping_obj = PdoMappingObject::new(&pdo);

        // Setup initially
        mapping_obj
            .write(1, &((0x1000 << 16) | 32 as u32).to_le_bytes())
            .unwrap();
        mapping_obj.write(0, &[1]).unwrap();
        comm_obj.write(1, &(1u32 << 31).to_le_bytes()).unwrap();

        nmt_state.store(NmtState::Operational);

        // Changing now should error
        let result = mapping_obj.write(1, &0u32.to_le_bytes());
        assert_eq!(Err(AbortCode::GeneralError), result);
        let result = comm_obj.write(1, &0u32.to_le_bytes());
        assert_eq!(Err(AbortCode::GeneralError), result);
        let result = comm_obj.write(2, &0u32.to_le_bytes());
        assert_eq!(Err(AbortCode::GeneralError), result);
    }

    /// A VAR object holding a single scalar field, mappable to both RPDOs and TPDOs
    struct VarObject<T> {
        data_type: DataType,
        value: T,
    }

    impl<T: SubObjectAccess> ProvidesSubObjects for VarObject<T> {
        fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
            let info = SubInfo {
                data_type: self.data_type,
                access_type: AccessType::Rw,
                pdo_mapping: PdoMappable::Both,
                persist: false,
            };
            match sub {
                0 => Some((info, &self.value)),
                _ => None,
            }
        }

        fn object_code(&self) -> ObjectCode {
            ObjectCode::Var
        }
    }

    /// Eight u64 objects at 0x2000..=0x2007 and one u16 object at 0x2100
    struct WideOd {
        u64_objects: [&'static VarObject<ScalarFieldU64>; 8],
        u16_object: &'static VarObject<ScalarFieldU16>,
    }

    impl WideOd {
        fn new() -> &'static Self {
            Box::leak(Box::new(Self {
                u64_objects: core::array::from_fn(|i| {
                    &*Box::leak(Box::new(VarObject {
                        data_type: DataType::UInt64,
                        value: ScalarFieldU64::new(0x0101_0101_0101_0101 * (i as u64 + 1)),
                    }))
                }),
                u16_object: Box::leak(Box::new(VarObject {
                    data_type: DataType::UInt16,
                    value: ScalarFieldU16::new(0xBEEF),
                })),
            }))
        }
    }

    impl ObjectLookup<'static> for WideOd {
        fn find_object(&self, index: u16) -> Option<&'static dyn ObjectAccess> {
            match index {
                0x2000..=0x2007 => Some(self.u64_objects[(index - 0x2000) as usize]),
                0x2100 => Some(self.u16_object),
                _ => None,
            }
        }
    }

    const TPDO_COB_ID: u32 = 0x181;

    /// Build a valid PDO for frame type `F` with the given default mappings
    fn wide_pdo<F: NodeFrame>(
        od: &'static WideOd,
        mappings: &'static [u32],
    ) -> &'static Pdo<'static, F> {
        let nmt_state = Box::leak(Box::new(AtomicCell::new(NmtState::PreOperational)));
        let data = Box::leak(Box::new(PdoData::<F>::default()));
        let defaults = Box::leak(Box::new(PdoDefaults::new(
            TPDO_COB_ID,
            false,
            false,
            true,
            false,
            254,
            mappings,
        )));
        let pdo = Box::leak(Box::new(Pdo::new(od, nmt_state, data, defaults)));
        pdo.init_defaults();
        pdo
    }

    const fn mapping(index: u16, bits: u8) -> u32 {
        ((index as u32) << 16) | bits as u32
    }

    #[test]
    fn test_fd_tpdo_carries_64_bytes() {
        const MAPPINGS: [u32; 8] = [
            mapping(0x2000, 64),
            mapping(0x2001, 64),
            mapping(0x2002, 64),
            mapping(0x2003, 64),
            mapping(0x2004, 64),
            mapping(0x2005, 64),
            mapping(0x2006, 64),
            mapping(0x2007, 64),
        ];
        let od = WideOd::new();
        let pdo = wide_pdo::<CanFdMessage>(od, &MAPPINGS);
        assert!(pdo.valid());

        pdo.send_pdo();
        let frame = pdo.take_frame().unwrap();
        assert_eq!(CanId::Std(TPDO_COB_ID as u16), frame.id());
        assert_eq!(64, frame.len());
        assert_eq!(64, frame.on_wire_len());
        let expected: Vec<u8> = (1..=8u8).flat_map(|i| [i; 8]).collect();
        assert_eq!(expected.as_slice(), frame.data());
        // The frame was consumed
        assert!(pdo.take_frame().is_none());
    }

    #[test]
    fn test_fd_tpdo_is_padded_to_legal_length() {
        const MAPPINGS: [u32; 2] = [mapping(0x2000, 64), mapping(0x2100, 16)];
        let od = WideOd::new();
        let pdo = wide_pdo::<CanFdMessage>(od, &MAPPINGS);

        pdo.send_pdo();
        let frame = pdo.take_frame().unwrap();
        assert_eq!(10, frame.len());
        assert_eq!(12, frame.on_wire_len());
        assert_eq!(
            &[1, 1, 1, 1, 1, 1, 1, 1, 0xEF, 0xBE, 0, 0],
            frame.padded_data()
        );
    }

    #[test]
    fn test_fd_rpdo_ignores_padding() {
        const MAPPINGS: [u32; 2] = [mapping(0x2000, 64), mapping(0x2100, 16)];
        let od = WideOd::new();
        let pdo = wide_pdo::<CanFdMessage>(od, &MAPPINGS);

        // 10 mapped bytes arrive padded to 12; the padding must not reach any object
        let data = [9, 9, 9, 9, 9, 9, 9, 9, 0x34, 0x12, 0xAA, 0xAA];
        let frame = CanFdMessage::new(CanId::Std(TPDO_COB_ID as u16), &data);
        assert!(pdo.receive_frame(&frame));
        let payload = pdo.data.buffered_value.take().unwrap();
        pdo.store_pdo_data(&payload);

        assert_eq!(0x0909_0909_0909_0909, od.u64_objects[0].value.load());
        assert_eq!(0x1234, od.u16_object.value.load());
        assert_eq!(0x0202_0202_0202_0202, od.u64_objects[1].value.load());
    }

    #[test]
    fn test_classic_pdo_rejects_more_than_8_bytes() {
        const MAPPINGS: [u32; 2] = [mapping(0x2000, 64), mapping(0x2100, 16)];
        let od = WideOd::new();
        let pdo = wide_pdo::<CanMessage>(od, &MAPPINGS);
        assert!(!pdo.valid());
        assert_eq!(0, pdo.data.valid_maps.load());

        // The same mappings fit a CAN FD frame
        let pdo = wide_pdo::<CanFdMessage>(od, &MAPPINGS);
        assert!(pdo.valid());
        assert_eq!(2, pdo.data.valid_maps.load());
    }

    #[test]
    fn test_classic_tpdo_unchanged() {
        const MAPPINGS: [u32; 2] = [mapping(0x2000, 32), mapping(0x2100, 16)];
        let od = WideOd::new();
        let pdo = wide_pdo::<CanMessage>(od, &MAPPINGS);

        pdo.send_pdo();
        let frame = pdo.take_frame().unwrap();
        assert_eq!(
            CanMessage::new(CanId::Std(TPDO_COB_ID as u16), &[1, 1, 1, 1, 0xEF, 0xBE]),
            frame
        );
    }

    fn read_cob_id<F: NodeFrame>(pdo: &'static Pdo<'static, F>) -> u32 {
        let comm = PdoCommObject::new(pdo);
        let mut buf = [0; 4];
        comm.read(1, 0, &mut buf).unwrap();
        u32::from_le_bytes(buf)
    }

    #[test]
    fn test_fd_pdo_has_no_rtr() {
        const MAPPINGS: [u32; 1] = [mapping(0x2000, 64)];
        let od = WideOd::new();
        const RTR_DISABLED: u32 = 1 << 30;

        // Defaults leave RTR enabled: a classic PDO keeps it, a CAN FD PDO cannot
        let classic = wide_pdo::<CanMessage>(od, &MAPPINGS);
        assert_eq!(0, read_cob_id(classic) & RTR_DISABLED);
        let fd = wide_pdo::<CanFdMessage>(od, &MAPPINGS);
        assert_eq!(RTR_DISABLED, read_cob_id(fd) & RTR_DISABLED);

        // Writing a COB-ID with RTR enabled does not enable it on CAN FD
        let comm = PdoCommObject::new(fd);
        comm.write(1, &0x281u32.to_le_bytes()).unwrap();
        assert_eq!(RTR_DISABLED | 0x281, read_cob_id(fd));
    }
}
