//! Core implementation of the LSS protocol
//!
//! This module includes definitions of all the relevant constants, and message serialization for
//! the Layer Setting Services (LSS) protocol. The LSS protocol is used for configuring the node ID
//! on unconfigured nodes, and for discovering the identity of unconfigured nodes.

use crate::messages::{CanId, CanMessage, MessageError, LSS_REQ_ID, LSS_RESP_ID};

/// Defines all possible values for the LSS command specifier field
#[derive(Debug, Clone, Copy)]
pub enum LssCommandSpecifier {
    /// Used to change the LSS mode for all nodes on the bus
    SwitchModeGlobal = 0x04,
    /// Used to set the Node ID of the node(s) currently in *Configuration* mode
    ConfigureNodeId = 0x11,
    /// Used to set the bit timing (baud rate) of the node(s) currently in *Configuration* mode
    ConfigureBitTiming = 0x13,
    /// Used to command nodes to activate a new bit rate setting
    ActivateBitTiming = 0x15,
    /// Used to command nodes to store their config (node ID and bit rate) persistently
    StoreConfiguration = 0x17,
    /// Sends Vendor ID for activating an LSS node via its identity
    SwitchStateVendor = 0x40,
    /// Sends Product Code for activating an LSS node via its identity
    SwitchStateProduct = 0x41,
    /// Sends Revision Number for activating an LSS node via its identity
    SwitchStateRev = 0x42,
    /// Sends Serial Number for activating an LSS node via its identity
    ///
    /// This command should come last (after vendor, product, rev), as a node which recognizes its
    /// own identity will respond on receipt of this message
    SwitchStateSerial = 0x43,
    /// Response by a node to indicate it has recognized its identity and is entering *Configuration* mode
    SwitchStateResponse = 0x44,
    /// Response to a FastScan message
    IdentifySlave = 0x4F,
    /// Message used for fast scan protocol to discover unconfigured nodes without knowing their identity
    FastScan = 0x51,
    /// Used to inquire the vendor ID of a node in *Configuration* mode
    InquireVendor = 0x5A,
    /// Used to inquire the product code of a node in *Configuration* mode
    InquireProduct = 0x5B,
    /// Used to inquire the revision number of a node in *Configuration* mode
    InquireRev = 0x5C,
    /// Used to inquire the serial number of a node in *Configuration* mode
    InquireSerial = 0x5D,
    /// Used to inquire the node ID of a node in *Configuration* mode
    InquireNodeId = 0x5E,
}

/// Represents the possible values of the error field returned in response to a ConfigureNodeId
/// command
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum LssConfigureError {
    /// Success
    Ok = 0,
    /// The node ID is not value (outside of range 1 to 127)
    NodeIdOutOfRange = 1,
    /// A manufacturer specific error is stored in the `spec_error` field
    Manufacturer = 0xff,
}

/// Special value for fastscan bit_check field
pub const LSS_FASTSCAN_CONFIRM: u8 = 0x80;

impl LssCommandSpecifier {
    /// Attempt to create an [`LssCommandSpecifier`] from a byte code
    pub fn from_byte(b: u8) -> Result<Self, MessageError> {
        match b {
            0x04 => Ok(Self::SwitchModeGlobal),
            0x11 => Ok(Self::ConfigureNodeId),
            0x13 => Ok(Self::ConfigureBitTiming),
            0x15 => Ok(Self::ActivateBitTiming),
            0x17 => Ok(Self::StoreConfiguration),
            0x40 => Ok(Self::SwitchStateVendor),
            0x41 => Ok(Self::SwitchStateProduct),
            0x42 => Ok(Self::SwitchStateRev),
            0x43 => Ok(Self::SwitchStateSerial),
            0x44 => Ok(Self::SwitchStateResponse),
            0x4F => Ok(Self::IdentifySlave),
            0x51 => Ok(Self::FastScan),
            0x5A => Ok(Self::InquireVendor),
            0x5B => Ok(Self::InquireProduct),
            0x5C => Ok(Self::InquireRev),
            0x5D => Ok(Self::InquireSerial),
            0x5E => Ok(Self::InquireNodeId),
            _ => Err(MessageError::UnexpectedLssCommand { value: b }),
        }
    }
}

/// An LSS request send by the master to the slave
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LssRequest {
    /// Switch the mode of all LSS slaves
    SwitchModeGlobal {
        /// The mode -- 0 = *Waiting*, 1 = *Configuring*
        mode: u8,
    },
    /// Set the node ID of the node currently in *Configuring* state
    ConfigureNodeId {
        /// The new node ID to set
        node_id: u8,
    },
    /// Set the bit time (baud rate) of the node currently in *Configuring* state
    ConfigureBitTiming {
        /// Defines what baudrate table is used to lookup the bit timing
        /// 0 means use the default table
        /// 1..127 are reserved
        /// 128..255 are user definable
        ///
        /// The default table is:
        /// - 0: 1MBit/s
        /// - 1: 800kBit/s
        /// - 2: 500kBit/s
        /// - 3: 250kBit/s
        /// - 4: 125kBit/s
        /// - 5: 100kBit/s
        /// - 6: 50kBit/s
        /// - 7: 20kBit/s
        /// - 8: 10kBit/s
        table: u8,
        /// The index into the baudrate table for the baudrate to select
        index: u8,
    },
    /// Store the currently configured node ID and bit timing to persistent storage
    StoreConfiguration,
    /// Command a new bitrate to be activated
    ActivateBitTiming {
        /// Duration in ms to delay before activating the new baudrate
        delay: u16,
    },
    /// Send the vendor ID to activate by idenity
    SwitchStateVendor {
        /// The vendor ID to match against (32-bit value)
        vendor_id: u32,
    },
    /// Send the producte code to activate by identity
    SwitchStateProduct {
        /// The product code to match against (32-bit value)
        product_code: u32,
    },
    /// Send the revision number to active by identity
    SwitchStateRevision {
        /// The revision number to match against (32-bit value)
        revision: u32,
    },
    /// Send the serial number to active by identity
    ///
    /// This should be sent last, as it triggers the slave to check its identity against the
    /// recieved values and respond if they match
    SwitchStateSerial {
        /// The serial number to match against (32-bit value)
        serial: u32,
    },
    /// Request the vendor ID from a node in LSS Configuring state
    InquireVendor,
    /// Request the product code from a node in LSS Configuring state
    InquireProduct,
    /// Request the revision from a node in LSS Configuring state
    InquireRev,
    /// Request the serial number from a node in LSS Configuring state
    InquireSerial,
    /// Request the node ID from a node in LSS Configuring state
    InquireNodeId,

    /// Send a FastScan query
    FastScan {
        /// The ID field
        id: u32,
        /// The bit_check field
        bit_check: u8,
        /// The sub index of the identity to check
        /// 0 - Vendor ID
        /// 1 - Product Code
        /// 2 - Revision
        /// 3 - Serial Number
        sub: u8,
        /// The sub index of the identity to check on the next FastScan request
        next: u8,
    },
}

impl TryFrom<&[u8]> for LssRequest {
    type Error = MessageError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(MessageError::MessageTooShort);
        }
        let cs = LssCommandSpecifier::from_byte(value[0])?;
        match cs {
            LssCommandSpecifier::SwitchModeGlobal => {
                if value.len() < 2 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::SwitchModeGlobal { mode: value[1] })
            }
            LssCommandSpecifier::ConfigureNodeId => {
                if value.len() < 2 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::ConfigureNodeId { node_id: value[1] })
            }
            LssCommandSpecifier::ConfigureBitTiming => {
                if value.len() < 3 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::ConfigureBitTiming {
                    table: value[1],
                    index: value[2],
                })
            }
            LssCommandSpecifier::ActivateBitTiming => {
                if value.len() < 3 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::ActivateBitTiming {
                    delay: u16::from_le_bytes([value[1], value[2]]),
                })
            }
            LssCommandSpecifier::StoreConfiguration => Ok(Self::StoreConfiguration),
            LssCommandSpecifier::SwitchStateVendor => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::SwitchStateVendor {
                    vendor_id: u32::from_le_bytes(value[1..5].try_into().unwrap()),
                })
            }
            LssCommandSpecifier::SwitchStateProduct => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::SwitchStateProduct {
                    product_code: u32::from_le_bytes(value[1..5].try_into().unwrap()),
                })
            }
            LssCommandSpecifier::SwitchStateRev => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::SwitchStateRevision {
                    revision: u32::from_le_bytes(value[1..5].try_into().unwrap()),
                })
            }
            LssCommandSpecifier::SwitchStateSerial => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::SwitchStateSerial {
                    serial: u32::from_le_bytes(value[1..5].try_into().unwrap()),
                })
            }
            LssCommandSpecifier::SwitchStateResponse => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::SwitchStateVendor {
                    vendor_id: u32::from_le_bytes(value[1..5].try_into().unwrap()),
                })
            }
            // IdentifySlave is only used in a response
            LssCommandSpecifier::IdentifySlave => {
                Err(MessageError::UnexpectedLssCommand { value: value[0] })
            }
            LssCommandSpecifier::FastScan => {
                if value.len() < 8 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::FastScan {
                    id: u32::from_le_bytes([value[1], value[2], value[3], value[4]]),
                    bit_check: value[5],
                    sub: value[6],
                    next: value[7],
                })
            }
            LssCommandSpecifier::InquireVendor => Ok(LssRequest::InquireVendor),
            LssCommandSpecifier::InquireProduct => Ok(LssRequest::InquireProduct),
            LssCommandSpecifier::InquireRev => Ok(LssRequest::InquireRev),
            LssCommandSpecifier::InquireSerial => Ok(LssRequest::InquireSerial),
            LssCommandSpecifier::InquireNodeId => Ok(LssRequest::InquireNodeId),
        }
    }
}

impl From<LssRequest> for CanMessage {
    fn from(value: LssRequest) -> Self {
        let mut data = [0u8; 8];
        match value {
            LssRequest::SwitchModeGlobal { mode } => {
                data[0] = LssCommandSpecifier::SwitchModeGlobal as u8;
                data[1] = mode;
            }
            LssRequest::ConfigureNodeId { node_id } => {
                data[0] = LssCommandSpecifier::ConfigureNodeId as u8;
                data[1] = node_id;
            }
            LssRequest::ConfigureBitTiming { table, index } => {
                data[0] = LssCommandSpecifier::ConfigureBitTiming as u8;
                data[1] = table;
                data[2] = index;
            }
            LssRequest::StoreConfiguration => {
                data[0] = LssCommandSpecifier::StoreConfiguration as u8;
            }
            LssRequest::ActivateBitTiming { delay } => {
                data[0] = LssCommandSpecifier::ActivateBitTiming as u8;
                let delay_bytes = delay.to_le_bytes();
                data[1] = delay_bytes[0];
                data[2] = delay_bytes[1];
            }
            LssRequest::SwitchStateVendor { vendor_id } => {
                data[0] = LssCommandSpecifier::SwitchStateVendor as u8;
                data[1..5].copy_from_slice(&vendor_id.to_le_bytes());
            }
            LssRequest::SwitchStateProduct { product_code } => {
                data[0] = LssCommandSpecifier::SwitchStateProduct as u8;
                data[1..5].copy_from_slice(&product_code.to_le_bytes());
            }
            LssRequest::SwitchStateRevision { revision } => {
                data[0] = LssCommandSpecifier::SwitchStateRev as u8;
                data[1..5].copy_from_slice(&revision.to_le_bytes());
            }
            LssRequest::SwitchStateSerial { serial } => {
                data[0] = LssCommandSpecifier::SwitchStateSerial as u8;
                data[1..5].copy_from_slice(&serial.to_le_bytes());
            }
            LssRequest::InquireVendor => {
                data[0] = LssCommandSpecifier::InquireVendor as u8;
            }
            LssRequest::InquireProduct => {
                data[0] = LssCommandSpecifier::InquireProduct as u8;
            }
            LssRequest::InquireRev => {
                data[0] = LssCommandSpecifier::InquireRev as u8;
            }
            LssRequest::InquireSerial => {
                data[0] = LssCommandSpecifier::InquireSerial as u8;
            }
            LssRequest::InquireNodeId => {
                data[0] = LssCommandSpecifier::InquireNodeId as u8;
            }
            LssRequest::FastScan {
                id,
                bit_check,
                sub,
                next,
            } => {
                data[0] = LssCommandSpecifier::FastScan as u8;
                data[1..5].copy_from_slice(&id.to_le_bytes());
                data[5] = bit_check;
                data[6] = sub;
                data[7] = next;
            }
        }
        CanMessage::new(LSS_REQ_ID, &data)
    }
}

/// An LSS response message sent from the Slave to Master
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LssResponse {
    /// Sent when a slave's identity matches a FastScan request
    IdentifySlave,
    /// Sent  in response to a [`LssRequest::SwitchStateSerial`] when it recognizes its
    /// identity
    SwitchStateResponse,
    /// Sent in response to a [`LssRequest::ConfigureNodeId`]
    ConfigureNodeIdAck {
        /// The error code
        error: u8,
        /// The manufacturer special error code
        ///
        /// Valid when error is 255, otherwise it's a don't care
        spec_error: u8,
    },
    /// Sent in response to a [`LssRequest::ConfigureBitTiming`]
    ConfigureBitTimingAck {
        /// The error code
        error: u8,
        /// The manufacturer special error code
        ///
        /// Valid when error is 255, otherwise it's a don't care
        spec_error: u8,
    },
    /// Sent in response to a [`LssRequest::StoreConfiguration`]
    StoreConfigurationAck {
        /// The error code
        error: u8,
        /// The manufacturer special error code
        ///
        /// Valid when error is 255, otherwise it's a don't care
        spec_error: u8,
    },
    /// Sent in response to a [`LssRequest::InquireVendor`]
    InquireVendorAck {
        /// The vendor id of the responding node
        vendor_id: u32,
    },
    /// Sent in response to a [`LssRequest::InquireProduct`]
    InquireProductAck {
        /// The product code of the responding node
        product_code: u32,
    },
    /// Sent in response to a [`LssRequest::InquireRev`]
    InquireRevAck {
        /// The revision number of the responding node
        revision: u32,
    },
    /// Sent in response to a [`LssRequest::InquireSerial`]
    InquireSerialAck {
        /// The serial number of the responding node
        serial_number: u32,
    },
    /// Sent in response to a [`LssRequest::InquireNodeId`]
    InquireNodeIdAck {
        /// The node ID of the responding node
        node_id: u8,
    },
}

impl TryFrom<&[u8]> for LssResponse {
    type Error = MessageError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(MessageError::MessageTooShort);
        }
        let cs = LssCommandSpecifier::from_byte(value[0])?;
        match cs {
            LssCommandSpecifier::IdentifySlave => Ok(Self::IdentifySlave {}),
            LssCommandSpecifier::SwitchStateResponse => Ok(Self::SwitchStateResponse {}),
            LssCommandSpecifier::ConfigureNodeId => {
                if value.len() < 3 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::ConfigureNodeIdAck {
                    error: value[1],
                    spec_error: value[2],
                })
            }
            LssCommandSpecifier::ConfigureBitTiming => {
                if value.len() < 3 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::ConfigureBitTimingAck {
                    error: value[1],
                    spec_error: value[2],
                })
            }
            LssCommandSpecifier::StoreConfiguration => {
                if value.len() < 3 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::StoreConfigurationAck {
                    error: value[1],
                    spec_error: value[2],
                })
            }
            LssCommandSpecifier::InquireVendor => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::InquireVendorAck {
                    vendor_id: u32::from_le_bytes([value[1], value[2], value[3], value[4]]),
                })
            }
            LssCommandSpecifier::InquireProduct => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::InquireProductAck {
                    product_code: u32::from_le_bytes([value[1], value[2], value[3], value[4]]),
                })
            }
            LssCommandSpecifier::InquireRev => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::InquireRevAck {
                    revision: u32::from_le_bytes([value[1], value[2], value[3], value[4]]),
                })
            }
            LssCommandSpecifier::InquireSerial => {
                if value.len() < 5 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::InquireSerialAck {
                    serial_number: u32::from_le_bytes([value[1], value[2], value[3], value[4]]),
                })
            }
            LssCommandSpecifier::InquireNodeId => {
                if value.len() < 2 {
                    return Err(MessageError::MessageTooShort);
                }
                Ok(Self::InquireNodeIdAck { node_id: value[1] })
            }
            _ => Err(MessageError::UnexpectedLssCommand { value: value[0] }),
        }
    }
}

impl TryFrom<CanMessage> for LssResponse {
    type Error = MessageError;

    fn try_from(value: CanMessage) -> Result<Self, Self::Error> {
        if value.id != LSS_RESP_ID {
            return Err(MessageError::UnexpectedId {
                cob_id: value.id,
                expected: LSS_RESP_ID,
            });
        }
        LssResponse::try_from(&value.data[..])
    }
}

impl LssResponse {
    /// Convert an LssReponse to a CanMessage
    pub fn to_can_message(self: &LssResponse, id: CanId) -> CanMessage {
        // LSS messages are required to always be 8 bytes long. For...some reason.
        let mut msg = CanMessage::new(id, &[0; 8]);
        match self {
            LssResponse::IdentifySlave => {
                msg.data[0] = LssCommandSpecifier::IdentifySlave as u8;
            }
            LssResponse::SwitchStateResponse => {
                msg.data[0] = LssCommandSpecifier::SwitchStateResponse as u8;
            }
            LssResponse::ConfigureNodeIdAck { error, spec_error } => {
                msg.data[0] = LssCommandSpecifier::ConfigureNodeId as u8;
                msg.data[1] = *error;
                msg.data[2] = *spec_error;
            }
            LssResponse::ConfigureBitTimingAck { error, spec_error } => {
                msg.data[0] = LssCommandSpecifier::ConfigureBitTiming as u8;
                msg.data[1] = *error;
                msg.data[2] = *spec_error;
            }
            LssResponse::StoreConfigurationAck { error, spec_error } => {
                msg.data[0] = LssCommandSpecifier::StoreConfiguration as u8;
                msg.data[1] = *error;
                msg.data[2] = *spec_error;
            }
            LssResponse::InquireVendorAck { vendor_id } => {
                msg.data[0] = LssCommandSpecifier::InquireVendor as u8;
                msg.data[1..5].copy_from_slice(&vendor_id.to_le_bytes());
            }
            LssResponse::InquireProductAck { product_code } => {
                msg.data[0] = LssCommandSpecifier::InquireProduct as u8;
                msg.data[1..5].copy_from_slice(&product_code.to_le_bytes());
            }
            LssResponse::InquireRevAck { revision } => {
                msg.data[0] = LssCommandSpecifier::InquireRev as u8;
                msg.data[1..5].copy_from_slice(&revision.to_le_bytes());
            }
            LssResponse::InquireSerialAck { serial_number } => {
                msg.data[0] = LssCommandSpecifier::InquireSerial as u8;
                msg.data[1..5].copy_from_slice(&serial_number.to_le_bytes());
            }
            LssResponse::InquireNodeIdAck { node_id } => {
                msg.data[0] = LssCommandSpecifier::InquireNodeId as u8;
                msg.data[1] = *node_id;
            }
        }
        msg
    }
}

/// The possible LSS states
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum LssState {
    /// The default state of a node.
    Waiting = 0,
    /// The state of a node which has been "activated" and is ready for configuring or querying via
    /// LSS
    Configuring = 1,
}

impl LssState {
    /// Create an LSS state from a mode byte
    pub fn from_byte(b: u8) -> Result<Self, MessageError> {
        match b {
            0x00 => Ok(Self::Waiting),
            0x01 => Ok(Self::Configuring),
            _ => Err(MessageError::InvalidField),
        }
    }
}

/// Represents the 128-bit identity in its four components
///
/// The node identity is stored in the 0x1018 record object, and it is used for the LSS protocol
///
/// The vendor_id, product_code, and revision are configured in the device config file. The serial
/// number must be set by the application to a unique value. This can be done, e.g., using a UID
/// register on the MCU, or by loading a previously programmed value from flash. It is important
/// that each device on the bus have a unique identity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LssIdentity {
    /// A number indicating the vendor of the device
    pub vendor_id: u32,
    /// A number indicating a product / model of the device
    pub product_code: u32,
    /// A number indicating the revision of the product
    pub revision: u32,
    /// A serial number which should be unique among all devices for a given vendor/product/revision
    /// combination
    pub serial: u32,
}

impl LssIdentity {
    /// Create a new LssIdentity object
    pub fn new(vendor_id: u32, product_code: u32, revision: u32, serial: u32) -> Self {
        Self {
            vendor_id,
            product_code,
            revision,
            serial,
        }
    }

    /// Read the LssIdentity by offset as if it were a [u32; 4] array
    pub fn by_addr(&self, addr: u8) -> u32 {
        match addr {
            0 => self.vendor_id,
            1 => self.product_code,
            2 => self.revision,
            3 => self.serial,
            _ => panic!("Invalid LSS identity address"),
        }
    }
}
