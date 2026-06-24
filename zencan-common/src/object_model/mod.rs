//! Object Definition
//!
//! Types for metadata describing the object dictionary and values stored in standard objects.

mod constants;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(feature = "std"))]
mod object_spec;
mod pdo;
mod read_size;
mod time_types;

pub use constants::*;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(feature = "std"))]
pub use object_spec::*;
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

/// An enum of all possible sub object data types
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DataType {
    Boolean,
    Int8,
    Int16,
    Int24,
    Int32,
    Int64,
    #[default]
    UInt8,
    UInt16,
    UInt24,
    UInt32,
    UInt64,
    Real32,
    Real64,
    VisibleString(usize),
    OctetString(usize),
    UnicodeString(usize),
    TimeOfDay,
    TimeDifference,
    Domain,
}

impl DataType {
    /// Returns true if the type is one of the stringy types
    pub fn is_str(&self) -> bool {
        matches!(
            self,
            DataType::VisibleString(_) | DataType::OctetString(_) | DataType::UnicodeString(_)
        )
    }

    /// Get the storage size of the data type
    pub fn size(&self) -> usize {
        match self {
            DataType::Boolean => 1,
            DataType::Int8 => 1,
            DataType::Int16 => 2,
            DataType::Int24 => 3,
            DataType::Int32 => 4,
            DataType::Int64 => 8,
            DataType::UInt8 => 1,
            DataType::UInt16 => 2,
            DataType::UInt24 => 3,
            DataType::UInt32 => 4,
            DataType::UInt64 => 8,
            DataType::Real32 => 4,
            DataType::Real64 => 8,
            DataType::VisibleString(size) => *size,
            DataType::OctetString(size) => *size,
            DataType::UnicodeString(size) => *size,
            DataType::TimeOfDay => TimeOfDay::SIZE,
            DataType::TimeDifference => TimeDifference::SIZE,
            DataType::Domain => 0, // Domain size is variable
        }
    }
}

/// Access type enum
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
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
