//! CAN frame types and transport traits.

mod adapter;
mod frame;
mod messages;
#[cfg(all(feature = "socketcan", target_os = "linux"))]
mod socketcan;
mod traits;

pub use adapter::{ClassicAdapter, ClassicAdapterSendError};
pub use frame::*;
pub use messages::*;
#[cfg(all(feature = "socketcan", target_os = "linux"))]
#[cfg_attr(docsrs, doc(all(feature = "socketcan", target_os = "linux")))]
pub use socketcan::{open_socketcan, open_socketcan_fd};
pub use traits::*;
