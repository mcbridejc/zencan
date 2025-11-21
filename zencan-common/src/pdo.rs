//! Definitions and data types related to PDOs

/// Represents a PDO mapping
///
/// Each mapping specifies one sub-object to be included in the PDO data bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "std",
    derive(serde::Deserialize),
    serde(deny_unknown_fields)
)]
pub struct PdoMapping {
    /// The object index
    pub index: u16,
    /// The object sub index
    pub sub: u8,
    /// The size of the object to map, in **bits**
    pub size: u8,
}

impl PdoMapping {
    /// Convert a PdoMapping object to the u32 representation stored in the PdoMapping object
    pub fn to_object_value(&self) -> u32 {
        ((self.index as u32) << 16) | ((self.sub as u32) << 8) | (self.size as u32)
    }

    /// Create a PdoMapping object from the raw u32 representation stored in the PdoMapping object
    pub fn from_object_value(value: u32) -> Self {
        let index = (value >> 16) as u16;
        let sub = ((value >> 8) & 0xff) as u8;
        let size = (value & 0xff) as u8;
        Self { index, sub, size }
    }
}
