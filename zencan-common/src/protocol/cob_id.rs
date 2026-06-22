//! Protocol CAN Message ID definitions
//!
//! CANOpen calls then COB-IDs, for "communications object identifier".

use crate::can::CanId;

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
