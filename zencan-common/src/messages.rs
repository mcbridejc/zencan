//! Message definitions

use snafu::Snafu;

use crate::{
    lss::{LssRequest, LssResponse},
    sdo::{SdoRequest, SdoResponse},
};

/// Yet another CanId enum
///
/// TODO: Consider if this should use the CanId from embedded_can?
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CanId {
    /// An extended 28-bit identifier
    Extended(u32),
    /// A std 11-bit identifier
    Std(u16),
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
    pub fn raw(&self) -> u32 {
        match self {
            CanId::Extended(id) => *id,
            CanId::Std(id) => *id as u32,
        }
    }

    /// Returns true if this ID is an extended ID
    pub fn is_extended(&self) -> bool {
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

/// The NMT state transition command specifier
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum NmtCommandSpecifier {
    /// Indicates device should transition to the Operation state
    Start = 1,
    /// Indicates device should transition to the Stopped state
    Stop = 2,
    /// Indicates device should transition to the PreOperational state
    EnterPreOp = 128,
    /// Indicates device should perform an application reset
    ResetApp = 129,
    /// Indicates device should perform a communications reset
    ResetComm = 130,
}

impl NmtCommandSpecifier {
    /// Create an NmtCommandCmd from the byte value transmitted in the message
    pub fn from_byte(b: u8) -> Result<Self, MessageError> {
        match b {
            1 => Ok(Self::Start),
            2 => Ok(Self::Stop),
            128 => Ok(Self::EnterPreOp),
            129 => Ok(Self::ResetApp),
            130 => Ok(Self::ResetComm),
            _ => Err(MessageError::InvalidField),
        }
    }
}

/// The COB ID used for sending NMT commands
pub const NMT_CMD_ID: CanId = CanId::Std(0);
/// The COB ID used for sending SYNC commands
pub const SYNC_ID: CanId = CanId::Std(0x80);
/// The COB ID used for LSS slave responses
pub const LSS_RESP_ID: CanId = CanId::Std(0x7E4);
/// The COB ID used for LSS master requests
pub const LSS_REQ_ID: CanId = CanId::Std(0x7E5);
/// The COB ID used for heartbeat messages
pub const HEARTBEAT_ID: u16 = 0x700;
/// The default base ID for sending SDO requests (server node ID is added)
pub const SDO_REQ_BASE: u16 = 0x600;
/// The default base ID for sending SDO responses (server node ID is added)
pub const SDO_RESP_BASE: u16 = 0x580;

/// An NmtCommand message
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NmtCommand {
    /// Specifies the type of command
    pub cs: NmtCommandSpecifier,
    /// Indicates the node it applies to. A node of 0 indicates a broadcast command to all nodes.
    pub node: u8,
}

impl TryFrom<CanMessage> for NmtCommand {
    type Error = MessageError;

    fn try_from(msg: CanMessage) -> Result<Self, Self::Error> {
        let payload = msg.data();
        if msg.id() != NMT_CMD_ID {
            Err(MessageError::UnexpectedId {
                cob_id: msg.id(),
                expected: NMT_CMD_ID,
            })
        } else if payload.len() >= 2 {
            let cmd = NmtCommandSpecifier::from_byte(payload[0])?;
            let node = payload[1];
            Ok(NmtCommand { cs: cmd, node })
        } else {
            Err(MessageError::MessageTooShort)
        }
    }
}

impl From<NmtCommand> for CanMessage {
    fn from(cmd: NmtCommand) -> Self {
        let mut msg = CanMessage {
            id: NMT_CMD_ID,
            dlc: 2,
            ..Default::default()
        };
        msg.data[0] = cmd.cs as u8;
        msg.data[1] = cmd.node;
        msg
    }
}

/// Possible NMT states for a node
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum NmtState {
    /// Bootup
    ///
    /// A node never remains in this state, as all nodes should transition automatically into PreOperational
    Bootup = 0,
    /// Node has been stopped
    Stopped = 4,
    /// Normal operational state
    Operational = 5,
    /// Node is awaiting command to enter operation
    PreOperational = 127,
}

impl core::fmt::Display for NmtState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NmtState::Bootup => write!(f, "Bootup"),
            NmtState::Stopped => write!(f, "Stopped"),
            NmtState::Operational => write!(f, "Operational"),
            NmtState::PreOperational => write!(f, "PreOperational"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
/// An error for [`NmtState::try_from()`]
pub struct InvalidNmtStateError(u8);

impl TryFrom<u8> for NmtState {
    type Error = InvalidNmtStateError;

    /// Attempt to convert a u8 to an NmtState enum
    ///
    /// Fails with BadNmtStateError if value is not a valid state
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use NmtState::*;
        match value {
            x if x == Bootup as u8 => Ok(Bootup),
            x if x == Stopped as u8 => Ok(Stopped),
            x if x == Operational as u8 => Ok(Operational),
            x if x == PreOperational as u8 => Ok(PreOperational),
            _ => Err(InvalidNmtStateError(value)),
        }
    }
}

/// A Heartbeat message
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Heartbeat {
    /// The ID of the node transmitting the heartbeat
    pub node: u8,
    /// A toggle value which is flipped on every heartbeat
    pub toggle: bool,
    /// The current NMT state of the node
    pub state: NmtState,
}

impl From<Heartbeat> for CanMessage {
    fn from(value: Heartbeat) -> Self {
        let mut msg = CanMessage {
            id: CanId::Std(HEARTBEAT_ID | value.node as u16),
            dlc: 1,
            ..Default::default()
        };
        msg.data[0] = value.state as u8;
        if value.toggle {
            msg.data[0] |= 1 << 7;
        }
        msg
    }
}
/// Represents a SYNC object/message
///
/// A single CAN node can serve as the SYNC provider, sending a periodic sync object to all other
/// nodes. The one byte count value starts at 1, and increments. On overflow, it should be reset to
/// 1.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SyncObject {
    count: u8,
}

impl SyncObject {
    /// Create a new SyncObjectd
    pub fn new(count: u8) -> Self {
        Self { count }
    }
}

impl Default for SyncObject {
    fn default() -> Self {
        Self { count: 1 }
    }
}

impl From<SyncObject> for CanMessage {
    fn from(value: SyncObject) -> Self {
        CanMessage::new(SYNC_ID, &[value.count])
    }
}

impl From<CanMessage> for SyncObject {
    fn from(msg: CanMessage) -> Self {
        if msg.id() == SYNC_ID {
            let count = msg.data()[0];
            Self { count }
        } else {
            panic!("Invalid message ID for SyncObject");
        }
    }
}

impl TryFrom<CanMessage> for ZencanMessage {
    type Error = MessageError;

    fn try_from(msg: CanMessage) -> Result<Self, Self::Error> {
        let cob_id = msg.id();
        if cob_id == NMT_CMD_ID {
            Ok(ZencanMessage::NmtCommand(msg.try_into()?))
        } else if cob_id.raw() & !0x7f == HEARTBEAT_ID as u32 {
            let node = (cob_id.raw() & 0x7f) as u8;
            let toggle = (msg.data[0] & (1 << 7)) != 0;
            let state: NmtState = (msg.data[0] & 0x7f)
                .try_into()
                .map_err(|e: InvalidNmtStateError| MessageError::InvalidNmtState { value: e.0 })?;
            Ok(ZencanMessage::Heartbeat(Heartbeat {
                node,
                toggle,
                state,
            }))
        } else if cob_id.raw() & 0xff80 == 0x580 {
            // SDO response
            let resp: SdoResponse = msg
                .try_into()
                .map_err(|_| MessageError::MalformedMsg { cob_id })?;
            Ok(ZencanMessage::SdoResponse(resp))
        } else if cob_id.raw() >= 0x580 && cob_id.raw() <= 0x580 + 256 {
            // SDO request
            let req: SdoRequest = msg
                .data()
                .try_into()
                .map_err(|_| MessageError::MalformedMsg { cob_id })?;
            Ok(ZencanMessage::SdoRequest(req))
        } else if cob_id == SYNC_ID {
            Ok(ZencanMessage::Sync(msg.into()))
        } else if cob_id == LSS_REQ_ID {
            let req: LssRequest = msg
                .data()
                .try_into()
                .map_err(|_| MessageError::MalformedMsg { cob_id })?;
            Ok(ZencanMessage::LssRequest(req))
        } else if cob_id == LSS_RESP_ID {
            let resp: LssResponse = msg
                .data()
                .try_into()
                .map_err(|_| MessageError::MalformedMsg { cob_id })?;
            Ok(ZencanMessage::LssResponse(resp))
        } else {
            Err(MessageError::UnrecognizedId { cob_id })
        }
    }
}

/// An enum representing all of the standard messages
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum ZencanMessage {
    NmtCommand(NmtCommand),
    Sync(SyncObject),
    Heartbeat(Heartbeat),
    SdoRequest(SdoRequest),
    SdoResponse(SdoResponse),
    LssRequest(LssRequest),
    LssResponse(LssResponse),
}

/// An error for problems converting CanMessages to zencan types
#[derive(Debug, Clone, Copy, PartialEq, Snafu)]
pub enum MessageError {
    /// Not enough bytes were present in the message
    MessageTooShort,
    /// The message was malformed in some way
    MalformedMsg {
        /// The COB ID of the malformed message
        cob_id: CanId,
    },
    /// The message ID was not the expected value
    #[snafu(display("Unexpected message ID found: {cob_id:?}, expected: {expected:?}"))]
    UnexpectedId {
        /// Received ID
        cob_id: CanId,
        /// Expected ID
        expected: CanId,
    },
    /// A field in the message contained an unallowed value for that field
    InvalidField,
    /// The COB ID of the message does not correspond to an expected ZencanMessag
    ///
    /// This isn't particular surprising, many messages on the bus will not (e.g. PDOs)
    UnrecognizedId {
        /// The unrecognized COB
        cob_id: CanId,
    },
    /// The NMT state integer in the message is not a valid NMT state
    InvalidNmtState {
        /// The invalid byte
        value: u8,
    },
    /// An invalid LSS command specifier was found in the message
    #[snafu(display("Unexpected LSS command: {value}"))]
    UnexpectedLssCommand {
        /// The invalid byte
        value: u8,
    },
}
