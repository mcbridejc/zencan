//! CAN Message definitions

use snafu::Snafu;

/// Yet another CanId enum
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CanId {
    /// An extended 28-bit identifier
    Extended(u32),
    /// A std 11-bit identifier
    Std(u16),
}

impl core::fmt::Display for CanId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CanId::Extended(id) => write!(f, "Extended(0x{id:x})"),
            CanId::Std(id) => write!(f, "Std(0x{id:x})"),
        }
    }
}

impl CanId {
    /// Create a new extended ID
    pub const fn extended(id: u32) -> CanId {
        CanId::Extended(id)
    }

    /// Create a new standard ID
    pub const fn std(id: u16) -> CanId {
        CanId::Std(id)
    }

    /// Get the raw ID as a u32
    pub const fn raw(&self) -> u32 {
        match self {
            CanId::Extended(id) => *id,
            CanId::Std(id) => *id as u32,
        }
    }

    /// Returns true if this ID is an extended ID
    pub const fn is_extended(&self) -> bool {
        match self {
            CanId::Extended(_) => true,
            CanId::Std(_) => false,
        }
    }
}

const MAX_DATA_LENGTH: usize = 8;

/// A struct to contain a CanMessage
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanMessage {
    /// The data payload of the message
    ///
    /// Note, some bytes may be unused. Check dlc.
    pub data: [u8; MAX_DATA_LENGTH],
    /// The length of the data payload
    pub dlc: u8,
    /// Indicates this message is a remote transmission request
    pub rtr: bool,
    /// The id of this message
    pub id: CanId,
}

impl Default for CanMessage {
    fn default() -> Self {
        Self {
            data: [0; MAX_DATA_LENGTH],
            dlc: 0,
            id: CanId::Std(0),
            rtr: false,
        }
    }
}

impl CanMessage {
    /// Create a new CAN message
    pub fn new(id: CanId, data: &[u8]) -> Self {
        let dlc = data.len() as u8;
        if dlc > MAX_DATA_LENGTH as u8 {
            panic!(
                "Data length exceeds maximum size of {} bytes",
                MAX_DATA_LENGTH
            );
        }
        let mut buf = [0u8; MAX_DATA_LENGTH];
        buf[0..dlc as usize].copy_from_slice(data);
        let rtr = false;

        Self {
            id,
            dlc,
            data: buf,
            rtr,
        }
    }

    /// Create a new RTR message
    ///
    /// RTR messages have no data payload
    pub fn new_rtr(id: CanId) -> Self {
        Self {
            id,
            rtr: true,
            ..Default::default()
        }
    }

    /// Get the id of the message
    pub fn id(&self) -> CanId {
        self.id
    }

    /// Get a slice containing the data payload
    pub fn data(&self) -> &[u8] {
        &self.data[0..self.dlc as usize]
    }

    /// Returns true if this message is a remote transmission request
    pub fn is_rtr(&self) -> bool {
        self.rtr
    }
}

/// The error codes which can be delivered in a CAN frame
///
/// These are set by a receiver when it detects an error in a received frame, and received globally
/// by all nodes on the bus
#[derive(Clone, Copy, Debug, Snafu)]
#[repr(u8)]
pub enum CanError {
    /// The transmitter detected a different value on the bus than the value is was transmitting at
    /// a point in the message after the arbitration process (sending of the ID)
    Bit = 1,
    /// A receiver detected a sequence of 6 bits of the same level, indicating a failure in bit
    /// stuffing
    Stuff = 2,
    /// A reveiver detected a malformed can frame (e.g. the SOF bit was not dominant, etc)
    Form = 3,
    /// The transmitter did not detect an ACK from any receivers
    Ack = 4,
    /// A receiver detected a mismatch in CRC value for the message
    Crc = 5,
    /// There are other bit patterns possible for the error field, but they have no defined meaning
    Other,
}

impl CanError {
    /// Create a CanError from the on-bus error code
    pub fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Bit,
            2 => Self::Stuff,
            3 => Self::Form,
            4 => Self::Ack,
            5 => Self::Crc,
            _ => Self::Other,
        }
    }
}
