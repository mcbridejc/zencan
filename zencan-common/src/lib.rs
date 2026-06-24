//! Common functionality shared among other zencan crates.
//!
//! Most users will have no reason to depend on this crate directly, as it is re-exported by both
//! `zencan-node` and `zencan-client`.
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs, missing_copy_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod atomic_cell;
pub use atomic_cell::AtomicCell;

pub mod can;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod node_configuration;
pub mod object_model;
pub mod protocol;

pub use arbitrary_int::{i24, u24};
#[cfg(all(feature = "socketcan", target_os = "linux"))]
#[cfg_attr(docsrs, doc(all(feature = "socketcan", target_os = "linux")))]
pub use can::open_socketcan;
pub use can::{CanError, CanId, CanMessage};
pub use object_model::{TimeDifference, TimeOfDay};
pub use protocol::NodeId;
