//! Definitions for the NMT protocol

use crate::can::CanMessage;

use super::{MessageError, NMT_CMD_ID};

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
pub struct InvalidNmtStateError(pub u8);

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
