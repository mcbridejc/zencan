use zencan_common::object_model::{AccessType, DataType, PdoMappable};

/// Information about a sub object
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SubInfo {
    /// The size (or max size) of this sub object, in bytes
    pub size: usize,
    /// The data type of this sub object
    pub data_type: DataType,
    /// Indicates what accesses (i.e. read/write) are allowed on this sub object
    pub access_type: AccessType,
    /// Indicates whether this sub may be mapped to PDOs
    pub pdo_mapping: PdoMappable,
    /// Indicates whether this sub should be persisted when data is saved
    pub persist: bool,
}

impl SubInfo {
    /// A shorthand value for sub0 on record and array objects
    pub const MAX_SUB_NUMBER: SubInfo = SubInfo {
        size: 1,
        data_type: DataType::UInt8,
        access_type: AccessType::Const,
        pdo_mapping: PdoMappable::None,
        persist: false,
    };

    /// Convenience function for creating a new sub-info by type
    pub const fn new_u32() -> Self {
        Self {
            size: 4,
            data_type: DataType::UInt32,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_u24() -> Self {
        Self {
            size: 3,
            data_type: DataType::UInt24,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_u16() -> Self {
        Self {
            size: 2,
            data_type: DataType::UInt16,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_u8() -> Self {
        Self {
            size: 1,
            data_type: DataType::UInt8,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_i32() -> Self {
        Self {
            size: 4,
            data_type: DataType::Int32,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_i24() -> Self {
        Self {
            size: 3,
            data_type: DataType::Int24,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_i16() -> Self {
        Self {
            size: 2,
            data_type: DataType::Int16,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_i8() -> Self {
        Self {
            size: 1,
            data_type: DataType::Int8,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_f32() -> Self {
        Self {
            size: 4,
            data_type: DataType::Real32,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_boolean() -> Self {
        Self {
            size: 1,
            data_type: DataType::Boolean,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function for creating a new sub-info by type
    pub const fn new_visible_str(size: usize) -> Self {
        Self {
            size,
            data_type: DataType::VisibleString,
            access_type: AccessType::Ro,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }
    }

    /// Convenience function to set the access_type to read-only
    pub const fn ro_access(mut self) -> Self {
        self.access_type = AccessType::Ro;
        self
    }

    /// Convenience function to set the access_type to read-write
    pub const fn rw_access(mut self) -> Self {
        self.access_type = AccessType::Rw;
        self
    }

    /// Convenience function to set the access_type to const
    pub const fn const_access(mut self) -> Self {
        self.access_type = AccessType::Const;
        self
    }

    /// Convenience function to set the access_type to write-only
    pub const fn wo_access(mut self) -> Self {
        self.access_type = AccessType::Wo;
        self
    }

    /// Convenience function to set the persist value
    pub const fn persist(mut self, value: bool) -> Self {
        self.persist = value;
        self
    }
}
