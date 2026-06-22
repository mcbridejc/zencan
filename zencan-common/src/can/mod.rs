//! CAN frame types and transport traits.

mod messages;
#[cfg(all(feature = "socketcan", target_os = "linux"))]
mod socketcan;
mod traits;

pub use messages::*;
#[cfg(all(feature = "socketcan", target_os = "linux"))]
#[cfg_attr(docsrs, doc(all(feature = "socketcan", target_os = "linux")))]
pub use socketcan::open_socketcan;
pub use traits::*;
