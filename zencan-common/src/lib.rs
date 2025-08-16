//! Common functionality shared among other zencan crates.
//!
//! Most users will have no reason to depend on this crate directly, as it is re-exported by both
//! `zencan-node` and `zencan-client`.
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs, missing_copy_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod atomic_cell;
pub use atomic_cell::AtomicCell;
pub mod constants;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod device_config;
pub mod lss;
pub mod messages;
pub mod node_id;
pub mod objects;
pub mod sdo;
pub mod traits;

#[cfg(feature = "socketcan")]
mod socketcan;

#[cfg(feature = "socketcan")]
#[cfg_attr(docsrs, doc(cfg(feature = "socketcan")))]
pub use socketcan::open_socketcan;

pub use node_id::NodeId;

pub use messages::{CanError, CanId, CanMessage};
