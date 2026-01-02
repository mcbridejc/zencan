//! Definitions for the NMT protocol

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
