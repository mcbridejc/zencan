use crate::can::CanMessage;

use super::SYNC_ID;

/// Represents a SYNC object/message
///
/// A single CAN node can serve as the SYNC provider, sending a periodic sync object to all other
/// nodes. When used, the one byte count value starts at 1, and increments. On overflow, it should be reset to
/// 1. It is optional, and when not provided the sync message DLC will be zero.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SyncObject {
    /// The (optional) count of this SyncObject
    pub count: Option<u8>,
}

impl SyncObject {
    /// Create a new SyncObjectd
    pub fn new(count: Option<u8>) -> Self {
        Self { count }
    }
}

impl From<SyncObject> for CanMessage {
    fn from(value: SyncObject) -> Self {
        if let Some(count) = value.count {
            CanMessage::new(SYNC_ID, &[count])
        } else {
            CanMessage::new(SYNC_ID, &[])
        }
    }
}

impl From<CanMessage> for SyncObject {
    fn from(msg: CanMessage) -> Self {
        if msg.id() == SYNC_ID {
            let count = if msg.data().is_empty() {
                None
            } else {
                Some(msg.data()[0])
            };
            Self { count }
        } else {
            panic!("Invalid message ID for SyncObject");
        }
    }
}
