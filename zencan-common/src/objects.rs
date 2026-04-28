//! Object Definitions
//!

/// A container for the address of a subobject
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectId {
    /// Object index
    pub index: u16,
    /// Sub index
    pub sub: u8,
}

/// Object Code value
///
/// Defines the type of an object or sub object
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ObjectCode {
    /// An empty object
    ///
    /// Zencan does not support Null objects
    Null = 0,
    /// A large chunk of data
    ///
    /// Zencan does not support Domain Object; it only supports domain sub-objects.
    Domain = 2,
    /// Unused
    DefType = 5,
    /// Unused
    DefStruct = 6,
    /// An object which has a single sub object
    #[default]
    Var = 7,
    /// An array of sub-objects all with the same data type
    Array = 8,
    /// A collection of sub-objects with varying types
    Record = 9,
}

impl TryFrom<u8> for ObjectCode {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ObjectCode::Null),
            2 => Ok(ObjectCode::Domain),
            5 => Ok(ObjectCode::DefType),
            6 => Ok(ObjectCode::DefStruct),
            7 => Ok(ObjectCode::Var),
            8 => Ok(ObjectCode::Array),
            9 => Ok(ObjectCode::Record),
            _ => Err(()),
        }
    }
}

/// Access type enum
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum AccessType {
    /// Read-only
    #[default]
    Ro,
    /// Write-only
    Wo,
    /// Read-write
    Rw,
    /// Read-only, and also will never be changed, even internally by the device
    Const,
}

impl AccessType {
    /// Returns true if an object with this access type can be read
    pub fn is_readable(&self) -> bool {
        matches!(self, AccessType::Ro | AccessType::Rw | AccessType::Const)
    }

    /// Returns true if an object with this access type can be written
    pub fn is_writable(&self) -> bool {
        matches!(self, AccessType::Rw | AccessType::Wo)
    }
}

/// Possible PDO mapping values for an object
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(
    feature = "std",
    derive(serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum PdoMappable {
    /// Object cannot be mapped to PDOs
    #[default]
    None,
    /// Object can be mapped to RPDOs only
    Rpdo,
    /// Object can be mapped to TPDOs only
    Tpdo,
    /// Object can be mapped to both RPDOs and TPDOs
    Both,
}

impl PdoMappable {
    /// Can be mapped to a TPDO
    pub fn supports_tpdo(&self) -> bool {
        matches!(self, PdoMappable::Tpdo | PdoMappable::Both)
    }

    /// Can be mapped to an RPDO
    pub fn supports_rpdo(&self) -> bool {
        matches!(self, PdoMappable::Rpdo | PdoMappable::Both)
    }
}

/// Indicate the type of data stored in an object
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[repr(u16)]
pub enum DataType {
    /// A true false value, encoded as a single byte, with 0 for false and 1 for true
    Boolean = 1,
    #[default]
    /// A signed 8-bit integer
    Int8 = 2,
    /// A signed 16-bit integer
    Int16 = 3,
    /// A signed 24-bit integer
    Int24 = 4,
    /// A signed 32-bit integer
    Int32 = 5,
    /// An unsigned 8-bit integer
    UInt8 = 6,
    /// An unsigned 16-bit integer
    UInt16 = 7,
    /// An unsigned 24-bit integer
    UInt24 = 8,
    /// An unsigned 32-bit integer
    UInt32 = 9,
    /// A 32-bit floating point value
    Real32 = 0xa,
    /// An ASCII/utf-8 string
    VisibleString = 0xb,
    /// A byte string
    OctetString = 0xc,
    /// A unicode string
    UnicodeString = 0xd,
    /// Currently Unimplemented
    TimeOfDay = 0xe,
    /// Currently Unimplemented
    TimeDifference = 0xf,
    /// An arbitrary byte access type for e.g. data streams, or large chunks of
    /// data. Size is typically not known at build time.
    Domain = 0x11,
    /// A 64-bit floating point value
    Real64 = 0x13,
    /// A signed 64-bit integer
    Int64 = 0x17,
    /// An unsigned 64-bit integer
    UInt64 = 0x1d,
    /// A contained for an unrecognized data type value
    Other(u16),
}

impl From<u16> for DataType {
    fn from(value: u16) -> Self {
        use DataType::*;
        match value {
            1 => Boolean,
            2 => Int8,
            3 => Int16,
            4 => Int24,
            5 => Int32,
            6 => UInt8,
            7 => UInt16,
            8 => UInt24,
            9 => UInt32,
            0xa => Real32,
            0xb => VisibleString,
            0xc => OctetString,
            0xd => UnicodeString,
            0x11 => Domain,
            _ => Other(value),
        }
    }
}

impl DataType {
    /// Returns true if data type is one of the string types
    pub fn is_str(&self) -> bool {
        matches!(
            self,
            Self::VisibleString | Self::OctetString | Self::UnicodeString
        )
    }
}

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
    pub const fn new_visibile_str(size: usize) -> Self {
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
