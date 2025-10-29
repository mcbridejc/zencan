//! A client for communicating with zencan nodes
//!
//! The crate provides utilities for communicating with nodes, including:
//!
//! - An [SDO client](SdoClient) for reading/writing a node's object dictionary via it's SDO server
//! - An [LSS master](LssMaster) for discovering and configuring un-configured nodes with IDs
//! - A [BusManager] which is intended to be the engine behind an application, such as `zencan-cli`,
//!   keeping track of nodes, and providing an API for managing them.
//! - Defining a [NodeConfig] TOML file format, which allows for storing and loading node configuration (primarily
//!   PDOs, but any objects can be written)
//!
//! This library is currently based on tokio/async. The plan is to also include blocking APIs in the
//! future.
//!
//! This should be considered very alpha, with important missing features, and potentially frequent
//! breaking API changes.
#![warn(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations
)]
#![allow(clippy::single_match)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod bus_manager;
mod lss_master;
pub mod nmt_master;
mod node_configuration;
mod sdo_client;
pub use zencan_common as common;

pub use bus_manager::BusManager;
pub use common::open_socketcan;
pub use lss_master::{LssError, LssMaster};
pub use node_configuration::{NodeConfig, PdoConfig, PdoMapping};
pub use sdo_client::{RawAbortCode, SdoClient, SdoClientError};
