//! Implements the standard 0x1018 identity object

use zencan_common::object_model::{AccessType, DataType, ObjectCode, PdoMappable};

use crate::object_dict::{ProvidesSubObjects, ScalarFieldU32, ScalarFieldU8, SubInfo};

/// Node Identity Object (0x1018)
///
/// This object has a custom implementation because it's values are never reset to defaults
#[allow(missing_debug_implementations)]
pub struct IdentityObject {
    vendor: ScalarFieldU32,
    product: ScalarFieldU32,
    revision: ScalarFieldU32,
    serial: ScalarFieldU32,
    max_sub: ScalarFieldU8,
}

impl IdentityObject {
    /// Create a new IdentityObject
    pub fn new(vendor: u32, product: u32, revision: u32, serial: u32) -> Self {
        Self {
            vendor: ScalarFieldU32::new(vendor),
            product: ScalarFieldU32::new(product),
            revision: ScalarFieldU32::new(revision),
            serial: ScalarFieldU32::new(serial),
            max_sub: ScalarFieldU8::new(4),
        }
    }

    /// Replace the default vendor ID with a new one
    pub fn set_vendor(&self, value: u32) {
        self.vendor.store(value);
    }
    /// Replace the default product ID with a new one
    pub fn set_product(&self, value: u32) {
        self.product.store(value);
    }
    /// Replace the default revision with a new one
    pub fn set_revision(&self, value: u32) {
        self.revision.store(value);
    }
    /// Replace the default serial with a new one
    pub fn set_serial(&self, value: u32) {
        self.serial.store(value);
    }

    /// Replace the default vendor ID with a new one
    pub fn get_vendor(&self) -> u32 {
        self.vendor.load()
    }
    /// Replace the default product ID with a new one
    pub fn get_product(&self) -> u32 {
        self.product.load()
    }
    /// Replace the default revision with a new one
    pub fn get_revision(&self) -> u32 {
        self.revision.load()
    }
    /// Replace the default serial with a new one
    pub fn get_serial(&self) -> u32 {
        self.serial.load()
    }
}

impl ProvidesSubObjects for IdentityObject {
    fn get_sub_object(
        &self,
        sub: u8,
    ) -> Option<(
        crate::object_dict::SubInfo,
        &dyn crate::object_dict::SubObjectAccess,
    )> {
        match sub {
            0 => Some((SubInfo::MAX_SUB_NUMBER, &self.max_sub)),
            1 => Some((
                SubInfo {
                    data_type: DataType::UInt32,
                    access_type: AccessType::Ro,
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
                &self.vendor,
            )),
            2 => Some((
                SubInfo {
                    data_type: DataType::UInt32,
                    access_type: AccessType::Ro,
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
                &self.product,
            )),
            3 => Some((
                SubInfo {
                    data_type: DataType::UInt32,
                    access_type: AccessType::Ro,
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
                &self.revision,
            )),
            4 => Some((
                SubInfo {
                    data_type: DataType::UInt32,
                    access_type: AccessType::Ro,
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
                &self.serial,
            )),
            _ => None,
        }
    }

    fn object_code(&self) -> ObjectCode {
        ObjectCode::Record
    }
}
