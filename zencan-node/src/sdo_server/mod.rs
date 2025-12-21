mod sdo_comms;
mod sdo_server;

pub(crate) use sdo_comms::SdoComms;
pub(crate) use sdo_server::SdoServer;

/// Default size for SDO data buffer
///
/// Enough for 127 segments of 7 bytes each, which is the maximum size of a block transfer
pub const SDO_BUFFER_SIZE: usize = 889;
