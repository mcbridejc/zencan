//! A library to implement the CANOpen open protocol in Rust
//!
//! Zencan-node is a library to implement communications between nodes on a CAN bus, modeled on the
//! CANOpen protocol. It is primarily intended to be used on small microcontrollers, and so is
//! no_std compatible, and performs no heap allocation, instead statically allocating most storage.
//! It is also possible to use it on std environments, for example on linux using socketcan. It
//! provides the following features:
//!
//! * Implements the *LSS* protocol for node discovery and configuration.
//! * Implements the NMT protocol for reporting and controlling the operating state of nodes.
//! * Generates an *object dictionary* to represent all of the data which can be communicated on the
//!   bus. This includes a number of standard communication objects, as well as application specific
//!   objects specified by the user.
//! * Implements an SDO server, allowing a client to access objects in the dictionary.
//! * Implements transmit and receive PDOs to allow mapping sets of objects to user-specified CAN
//!   IDs for reading and writing those objects, so that a change in an object on one node can be
//!   reflected in another object on another node.
//! * Provides callback hooks to allow for persistent storage of selected object values on command.
//!
//! # Getting Started
//!
//! ## Device Configuration
//!
//! A zencan node is configured using a [DeviceConfig](common::DeviceConfig) TOML file, see
//! [common::device_config] module docs.
//!
//! ## Code Generation
//!
//! The device configuration is used to generate types and static instances for each object in the
//! object dictionary, as well as some additional types like a [NodeMbox], and [NodeState].
//!
//! First, add `zencan-build` to your build deps:
//!
//! ```toml
//! [build-dependencies]
//! zencan-build = "0.0.1"
//! ```
//!
//! Then, add something like this to your project's `build.rs` file:
//!
//! ```ignore
//! fn main() {
//!     if let Err(e) = zencan_build::build_node_from_device_config("ZENCAN_CONFIG", "zencan_config.toml") {
//!         eprintln!("Failed to parse zencan_config.toml: {}", e.to_string());
//!         std::process::exit(-1);
//!     }
//! }
//! ```
//!
//! Then, include the generated code somewhere in your application, e.g. in `main.rs`:
//!
//! ```ignore
//! mod zencan {
//!     zencan_node::include_modules!(ZENCAN_CONFIG);
//! }
//! ```
//!
//! ## Instantiating the [`Node`] object
//!
//! The node object has to be created in two steps, using the statics created by the
//! `include_modules!`. The first step initializes the object dictionary -- mainly it registers
//! object callbacks so that the callback objects such as PDO config objects can be written to. The
//! second step instantiates the node and latches some of the configuration from the object
//! dictionary. In between these two steps is where the application should make sure that any
//! run-time loaded object values are stored -- for example this is the time to read back any object
//! values which have been stored in flash, or to configure the device serial number.
//!
//! ```ignore
//! // Read saved node ID from flash
//! let node_id = read_saved_node_id(&mut flash);
//!
//! // Initialize node, providing references to the static objects created by `zencan-build`
//! let initnode = Node::init(
//!     node_id,
//!     &zencan::NODE_MBOX,
//!     &zencan::NODE_STATE,
//!     &zencan::OD_TABLE,
//! );
//!
//! // Use the UID register to set a unique serial number
//! zencan::OBJECT1018.set_serial(get_serial());
//!
//! // Restore object values from a previous save. The source data is the slice of bytes provided by
//! // the node callback to be saved. The application is responsible for storing this somewhere
//! // (e.g. flash)  and returning it
//! let serialized_object_data: &[u8] = get_object_data();
//! restore_stored_objects(&zencan::OD_TABLE, serialized_object_data);
//!
//! // Perform the finalize step to get a ready-to-execute node
//! let mut node = initnode.finalize();
//! ```
//!
//! ## Handling CAN messages
//!
//! The application has to handle sending and receiving CAN messages.
//!
//! Received messages should be passed to the `NODE_MBOX` struct. This can be done in any thread --
//! a common way to do it is to have the CAN controller receive interrupt store messages here
//! directly.
//!
//! ```ignore
//! let msg = zencan_node::common::messages::CanMessage::new(id, &buffer[..msg.len as usize]);
//! // Ignore error -- as an Err is returned for messages that are not consumed by the node
//! // stack
//! zencan::NODE_MBOX.store_message(msg).ok();
//! ```
//!
//! To execute the Node logic, the [`Node::process`] function must be called periodically. It is
//! provided a callback for transmitting messages. While it is possible to call process only
//! periodically, the NODE_MBOX provides a callback which can be used to notify another task that
//! process should be called when a message is received and requires processing.
//!
//! Here's an example of a lilos task which executes process when either CAN_NOTIFY is signals, or
//! 10ms has passed since the last notification.
//!
//! ```ignore
//! async fn can_task(
//!     mut node: Node,
//!     mut can_tx: fdcan::Tx<FdCan1, NormalOperationMode>,
//! ) -> Infallible {
//!     let epoch = lilos::time::TickTime::now();
//!     loop {
//!         lilos::time::with_timeout(Duration::from_millis(10), CAN_NOTIFY.until_next()).await;
//!         let time_us = epoch.elapsed().0 * 1000;
//!         node.process(time_us, &mut |msg| {
//!             let header = zencan_to_fdcan_header(&msg);
//!             if let Err(_) = can_tx.transmit(header, msg.data()) {
//!                 defmt::error!("Error transmitting CAN message");
//!             }
//!         });
//!     }
//! }
//! ```
//!
//! ## Register callbacks
//!
//! The application can register callbacks for persistently storing data, or notifying the
//! processing task. See examples for more info.
//!
#![cfg_attr(all(not(test), not(feature = "std")), no_std)]
#![warn(missing_docs, missing_debug_implementations)]
#![allow(clippy::comparison_chain)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod bootloader;
mod lss_slave;
mod node;
mod node_mbox;
mod node_state;
pub mod object_dict;
pub mod pdo;
mod persist;
mod sdo_server;
pub mod storage;

// Re-export proc macros
pub use zencan_macro::build_object_dict;

// Re-export types used by generated code
pub use critical_section;
pub use heapless;
pub use zencan_common as common;

pub use bootloader::{BootloaderInfo, BootloaderSection, BootloaderSectionCallbacks};
#[cfg(feature = "socketcan")]
#[cfg_attr(docsrs, doc(cfg(feature = "socketcan")))]
pub use common::open_socketcan;
pub use node::Node;
pub use node_mbox::NodeMbox;
pub use node_state::{NodeState, NodeStateAccess};
pub use persist::restore_stored_objects;
pub use sdo_server::SDO_BUFFER_SIZE;

/// Include the code generated for the object dict in the build script.
#[macro_export]
macro_rules! include_modules {
    ($name: tt) => {
        include!(env!(
            concat!("ZENCAN_INCLUDE_GENERATED_", stringify!($name),),
            concat!(
                "Missing env var ",
                "ZENCAN_INCLUDE_GENERATED_",
                stringify!($name),
                ". Did you generate an object dictionary named ",
                stringify!($name),
                " in build.rs?"
            )
        ));
    };
}
