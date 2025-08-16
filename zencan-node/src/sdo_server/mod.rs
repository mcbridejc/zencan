mod sdo_receiver;
mod sdo_server;

pub(crate) use sdo_receiver::SdoReceiver;
pub(crate) use sdo_server::SdoServer;

/// Enough for 127 segments of 7 bytes7
pub const SDO_BUFFER_SIZE: usize = 889;
