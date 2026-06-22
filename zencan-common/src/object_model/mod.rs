//! Object Definition
//!
//! Types for metadata describing the object dictionary and values stored in standard objects.

mod constants;
mod pdo;
mod read_size;
mod time_types;

pub use constants::*;
pub use pdo::*;
pub use read_size::*;
pub use time_types::*;

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

impl TryFrom<&str> for AccessType {
    type Error = ();

    /// Attempts to create `AccessType` from lowercase str.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        use AccessType::*;
        match value {
            "ro" => Ok(Ro),
            "wo" => Ok(Wo),
            "rw" => Ok(Rw),
            "const" => Ok(Const),
            _ => Err(()),
        }
    }
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
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum DataType {
    /// A true false value, encoded as a single byte, with 0 for false and 1 for true
    Boolean = 1,
    #[default]
    /// A signed 8-bit integer
    Int8 = 2,
    /// A signed 16-bit integer
    Int16 = 3,
    /// A signed 32-bit integer
    Int32 = 4,
    /// An unsigned 8-bit integer
    UInt8 = 5,
    /// An unsigned 16-bit integer
    UInt16 = 6,
    /// An unsigned 32-bit integer
    UInt32 = 7,
    /// A 32-bit floating point value
    Real32 = 0x8,
    /// An ASCII/utf-8 string
    VisibleString = 0x9,
    /// A byte string
    OctetString = 0xa,
    /// A unicode string
    UnicodeString = 0xb,
    /// Currently Unimplemented
    TimeOfDay = 0xc,
    /// Currently Unimplemented
    TimeDifference = 0xd,
    /// An arbitrary byte access type for e.g. data streams, or large chunks of
    /// data. Size is typically not known at build time.
    Domain = 0xf,
    /// A signed 24-bit integer
    Int24 = 0x10,
    /// A 64-bit floating point value
    Real64 = 0x11,
    /// A signed 64-bit integer
    Int64 = 0x15,
    /// An unsigned 24-bit integer
    UInt24 = 0x16,
    /// An unsigned 64-bit integer
    UInt64 = 0x1b,
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
            4 => Int32,
            5 => UInt8,
            6 => UInt16,
            7 => UInt32,
            8 => Real32,
            9 => VisibleString,
            0xa => OctetString,
            0xb => UnicodeString,
            0xf => Domain,
            0x10 => Int24,
            0x11 => Real64,
            0x15 => Int64,
            0x16 => UInt24,
            0x1b => UInt64,
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
