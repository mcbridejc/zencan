//! Implementation of PDO configuration objects and PDO transmission
//!

use crate::object_dict::{
    find_object_entry, ConstField, ODEntry, ObjectAccess, ProvidesSubObjects, SubObjectAccess,
};
use zencan_common::{
    objects::{AccessType, DataType, ObjectCode, PdoMapping, SubInfo},
    sdo::AbortCode,
    AtomicCell, CanId,
};

/// Specifies the number of mapping parameters supported per PDO
///
/// Since we do not yet support CAN-FD, or sub-byte mapping, it's not possible to map more than 8
/// objects to a single PDO
const N_MAPPING_PARAMS: usize = 8;

#[derive(Clone, Copy)]
struct MappingEntry {
    object: &'static ODEntry<'static>,
    sub: u8,
    length: u8,
}

/// Represents a single PDO state
#[allow(missing_debug_implementations)]
pub struct Pdo {
    /// The COB-ID used to send or receive this PDO
    cob_id: AtomicCell<CanId>,
    /// Indicates if the PDO is enabled
    valid: AtomicCell<bool>,
    /// If set, this PDO cannot be requested via RTR
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
    /// The last received data value for an RPDO
    pub buffered_value: AtomicCell<Option<[u8; 8]>>,
    /// Indicates how many of the values in mapping_params are valid
    ///
    /// This represents sub0 for the mapping object
    valid_maps: AtomicCell<u8>,
    /// The mapping parameters
    ///
    /// These specify which objects are
    mapping_params: [AtomicCell<Option<MappingEntry>>; N_MAPPING_PARAMS],
}

impl Default for Pdo {
    fn default() -> Self {
        Self::new()
    }
}

impl Pdo {
    /// Create a new PDO object
    pub const fn new() -> Self {
        let cob_id = AtomicCell::new(CanId::Std(0));
        let valid = AtomicCell::new(false);
        let rtr_disabled = AtomicCell::new(false);
        let transmission_type = AtomicCell::new(0);
        let sync_counter = AtomicCell::new(0);
        let buffered_value = AtomicCell::new(None);
        let valid_maps = AtomicCell::new(0);
        let mapping_params = [const { AtomicCell::new(None) }; N_MAPPING_PARAMS];
        Self {
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

    /// Set the valid bit
    pub fn set_valid(&self, value: bool) {
        self.valid.store(value);
    }

    /// Get the valid bit value
    pub fn valid(&self) -> bool {
        self.valid.load()
    }

    /// Set the transmission type for this PDO
    pub fn set_transmission_type(&self, value: u8) {
        self.transmission_type.store(value);
    }

    /// Get the transmission type for this PDO
    pub fn transmission_type(&self) -> u8 {
        self.transmission_type.load()
    }

    /// Set the COB used for transmission of this PDO
    pub fn set_cob_id(&self, value: CanId) {
        self.cob_id.store(value)
    }

    /// Get the COB used for transmission of this PDO
    pub fn cob_id(&self) -> CanId {
        self.cob_id.load()
    }

    /// This function should be called when a SYNC event occurs
    ///
    /// It will return true if the PDO should be sent in response to the SYNC event
    pub fn sync_update(&self) -> bool {
        if !self.valid.load() {
            return false;
        }

        let transmission_type = self.transmission_type.load();
        if transmission_type == 0 {
            // TODO: Figure out how to determine application "event" which triggers the PDO
            // For now, send every sync
            true
        } else if transmission_type <= 240 {
            let cnt = self.sync_counter.fetch_add(1) + 1;
            cnt == transmission_type
        } else {
            false
        }
    }

    /// Check mapped objects for TPDO event flag
    pub fn read_events(&self) -> bool {
        if !self.valid.load() {
            return false;
        }

        for i in 0..self.mapping_params.len() {
            let param = self.mapping_params[i].load();
            if param.is_none() {
                break;
            }
            let param = param.unwrap();
            if param.object.data.read_event_flag(param.sub) {
                return true;
            }
        }
        false
    }

    pub(crate) fn clear_events(&self) {
        for i in 0..self.mapping_params.len() {
            let param = self.mapping_params[i].load();
            if param.is_none() {
                break;
            }
            let param = param.unwrap();
            param.object.data.clear_events();
        }
    }

    pub(crate) fn store_pdo_data(&self, data: &[u8]) {
        let mut offset = 0;
        let valid_maps = self.valid_maps.load() as usize;
        for (i, param) in self.mapping_params.iter().enumerate() {
            if i >= valid_maps {
                break;
            }
            let param = param.load();
            if param.is_none() {
                break;
            }
            let param = param.unwrap();
            let length = param.length as usize;
            if offset + length > data.len() {
                break;
            }
            let data_to_write = &data[offset..offset + length];
            // validity of the mappings must be validated during write, so that error here is not
            // possible
            param.object.data.write(param.sub, data_to_write).ok();
            offset += length;
        }
    }

    pub(crate) fn read_pdo_data(&self, data: &mut [u8]) {
        let mut offset = 0;
        let valid_maps = self.valid_maps.load() as usize;
        for (i, param) in self.mapping_params.iter().enumerate() {
            if i >= valid_maps {
                break;
            }
            let param = param.load();
            // The first N params will be valid. Can assume if one is None, all remaining will be as
            // well
            if param.is_none() {
                break;
            }
            let param = param.unwrap();
            let length = param.length as usize;
            if offset + length > data.len() {
                break;
            }
            // validity of the mappings must be validated during write, so that error here is not
            // possible
            param
                .object
                .data
                .read(param.sub, 0, &mut data[offset..offset + length])
                .ok();
            offset += length;
        }
    }
}

struct PdoCobSubObject {
    pdo: &'static Pdo,
}

impl PdoCobSubObject {
    pub const fn new(pdo: &'static Pdo) -> Self {
        Self { pdo }
    }
}

impl SubObjectAccess for PdoCobSubObject {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let cob_id = self.pdo.cob_id.load();
        let mut value = cob_id.raw();
        if cob_id.is_extended() {
            value |= 1 << 29;
        }
        if self.pdo.rtr_disabled.load() {
            value |= 1 << 30;
        }
        if !self.pdo.valid.load() {
            value |= 1 << 31;
        }

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
        if data.len() < 4 {
            Err(AbortCode::DataTypeMismatchLengthLow)
        } else if data.len() > 4 {
            Err(AbortCode::DataTypeMismatchLengthHigh)
        } else {
            let value = u32::from_le_bytes(data.try_into().unwrap());
            let not_valid = (value & (1 << 31)) != 0;
            let no_rtr = (value & (1 << 30)) != 0;
            let extended_id = (value & (1 << 29)) != 0;

            let can_id = if extended_id {
                CanId::Extended(value & 0x1FFFFFFF)
            } else {
                CanId::Std((value & 0x7FF) as u16)
            };
            self.pdo.cob_id.store(can_id);
            self.pdo.valid.store(!not_valid);
            self.pdo.rtr_disabled.store(no_rtr);
            Ok(())
        }
    }
}

struct PdoTransmissionTypeSubObject {
    pdo: &'static Pdo,
}

impl PdoTransmissionTypeSubObject {
    pub const fn new(pdo: &'static Pdo) -> Self {
        Self { pdo }
    }
}

impl SubObjectAccess for PdoTransmissionTypeSubObject {
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
pub struct PdoCommObject {
    cob: PdoCobSubObject,
    transmission_type: PdoTransmissionTypeSubObject,
}

impl PdoCommObject {
    /// Create a new PdoCommObject
    pub const fn new(pdo: &'static Pdo) -> Self {
        let cob = PdoCobSubObject::new(pdo);
        let transmission_type = PdoTransmissionTypeSubObject::new(pdo);
        Self {
            cob,
            transmission_type,
        }
    }
}

impl ProvidesSubObjects for PdoCommObject {
    fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
        match sub {
            0 => Some((
                SubInfo::MAX_SUB_NUMBER,
                const { &ConstField::new(2u8.to_le_bytes()) },
            )),
            1 => Some((SubInfo::new_u32().rw_access().persist(true), &self.cob)),
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
pub struct PdoMappingObject {
    od: &'static [ODEntry<'static>],
    pdo: &'static Pdo,
}

impl PdoMappingObject {
    /// Create a new PdoMappingObject
    pub const fn new(od: &'static [ODEntry<'static>], pdo: &'static Pdo) -> Self {
        Self { od, pdo }
    }
}

impl ObjectAccess for PdoMappingObject {
    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        if sub == 0 {
            if offset < 1 && !buf.is_empty() {
                buf[0] = self.pdo.valid_maps.load();
                Ok(1)
            } else {
                Ok(0)
            }
        } else if sub <= self.pdo.mapping_params.len() as u8 {
            let value = if let Some(param) = self.pdo.mapping_params[(sub - 1) as usize].load() {
                ((param.object.index as u32) << 16)
                    + ((param.sub as u32) << 8)
                    + param.length as u32 * 8
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
        if sub == 0 {
            self.pdo.valid_maps.store(data[0]);
            Ok(())
        } else if sub <= self.pdo.mapping_params.len() as u8 {
            if data.len() != 4 {
                return Err(AbortCode::DataTypeMismatch);
            }
            let value = u32::from_le_bytes(data.try_into().unwrap());

            let object_id = (value >> 16) as u16;
            let mapping_sub = ((value & 0xFF00) >> 8) as u8;
            // Rounding up to BYTES, because we do not currently support bit access
            let length = (value & 0xFF) as usize;
            if (length % 8) != 0 {
                // only support byte level access for now
                return Err(AbortCode::IncompatibleParameter);
            }
            let length = length / 8;
            let entry = find_object_entry(self.od, object_id).ok_or(AbortCode::NoSuchObject)?;
            let sub_info = entry.data.sub_info(mapping_sub)?;
            if sub_info.size < length {
                return Err(AbortCode::IncompatibleParameter);
            }
            self.pdo.mapping_params[(sub - 1) as usize].store(Some(MappingEntry {
                object: entry,
                sub: mapping_sub,
                length: length as u8,
            }));
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
                size: 1,
                data_type: DataType::UInt8,
                access_type: AccessType::Rw,
                pdo_mapping: PdoMapping::None,
                persist: true,
            })
        } else if sub <= self.pdo.mapping_params.len() as u8 {
            Ok(SubInfo {
                size: 4,
                data_type: DataType::UInt32,
                access_type: AccessType::Rw,
                pdo_mapping: PdoMapping::None,
                persist: true,
            })
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }
}
