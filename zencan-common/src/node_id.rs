//! Types for representing node IDs
//!

/// An enum representing the node ID of a CANopen node. The node ID must be between 1 and 127 for
/// configured devices, with the special value of 255 used to represent an unconfigured device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum NodeId {
    /// A special node ID indicating the node is not configured (255)
    Unconfigured,
    /// A valid node ID for a configured node
    Configured(ConfiguredNodeId),
}

/// A newtype on u8 to enforce valid node ID (1-127)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ConfiguredNodeId(u8);
impl ConfiguredNodeId {
    /// Try to create a new ConfiguredId
    ///
    /// It will fail if value is invalid (i.e. <1 or >127)
    pub const fn new(value: u8) -> Result<Self, InvalidNodeIdError> {
        if (value > 0 && value < 128) || value == 255 {
            Ok(ConfiguredNodeId(value))
        } else {
            Err(InvalidNodeIdError)
        }
    }

    /// Get the raw node ID as a u8
    pub fn raw(&self) -> u8 {
        self.0
    }
}

impl core::fmt::Display for ConfiguredNodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<ConfiguredNodeId> for u8 {
    fn from(value: ConfiguredNodeId) -> Self {
        value.raw()
    }
}

impl NodeId {
    /// Try to create a new NodeId from a u8
    ///
    /// Will fail if the value is not a valid node ID
    pub const fn new(value: u8) -> Result<Self, InvalidNodeIdError> {
        if value == 255 {
            Ok(NodeId::Unconfigured)
        } else {
            match ConfiguredNodeId::new(value) {
                Ok(id) => Ok(NodeId::Configured(id)),
                Err(e) => Err(e),
            }
        }
    }

    /// Get the raw node ID as a u8
    pub fn raw(&self) -> u8 {
        match self {
            NodeId::Unconfigured => 255,
            NodeId::Configured(node_id_num) => node_id_num.0,
        }
    }

    /// Attempt to get the node ID as a ConfiguredNodeId
    ///
    /// Returns none if the value is NodeId::Unconfigured
    pub fn as_configured(&self) -> Option<ConfiguredNodeId> {
        match self {
            NodeId::Unconfigured => None,
            NodeId::Configured(configured_node_id) => Some(*configured_node_id),
        }
    }

    /// Return true if the NodeId contains a valid configured ID
    pub fn is_configured(&self) -> bool {
        match self {
            Self::Configured(_) => true,
            Self::Unconfigured => false,
        }
    }
    /// Return true if the node ID is NodeId::Unconfigured
    pub fn is_unconfigured(&self) -> bool {
        match self {
            Self::Configured(_) => false,
            Self::Unconfigured => true,
        }
    }
}

/// Error for converting u8 to a NodeId
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidNodeIdError;

impl core::fmt::Display for InvalidNodeIdError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Invalid node ID")
    }
}
impl core::error::Error for InvalidNodeIdError {}

impl TryFrom<u8> for NodeId {
    type Error = InvalidNodeIdError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value == 255 {
            Ok(NodeId::Unconfigured)
        } else {
            Ok(NodeId::Configured(ConfiguredNodeId(value)))
        }
    }
}

impl From<NodeId> for u8 {
    fn from(value: NodeId) -> Self {
        value.raw()
    }
}
