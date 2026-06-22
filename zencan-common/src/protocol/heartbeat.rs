use crate::can::{CanId, CanMessage};

use super::{NmtState, HEARTBEAT_ID};

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
