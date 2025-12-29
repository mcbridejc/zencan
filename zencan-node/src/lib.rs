//! A library to implement a CANOpen node in Rust
//!
//! Zencan-node is a library to implement CAN communications for an embedded node, using the CANOpen
//! protocol. It is primarily intended to be run on microcontrollers, and so it is no_std compatible
//! and performs no heap allocation, instead statically allocating storage. It is also possible to
//! use it on std environments, for example on linux using socketcan. It provides the following
//! features:
//!
//! * Implements the *LSS* protocol for node discovery and configuration.
//! * Implements the *NMT* protocol for reporting and controlling the operating state of nodes.
//! * Generates an *object dictionary* to represent all of the data which can be communicated on the
//!   bus. This includes a number of standard communication objects, as well as application specific
//!   objects specified by the user.
//! * Implements an *SDO* server, allowing a remote client to access objects in the dictionary.
//! * Implements transmit and receive PDOs, allowing the mapping of objects to user-specified CAN
//!   IDs for reading and writing those objects.
//! * Provides callback hooks to allow for persistent storage of selected object values on command.
//!
//! # Getting Started
//!
//! ## Device Configuration
//!
//! A zencan node is configured using a [DeviceConfig](common::device_config::DeviceConfig) TOML
//! file, see [common::device_config] module docs for more info.
//!
//! ## Code Generation
//!
//! The device configuration is used to generate types and static instances for each object in the
//! object dictionary, as well as some additional objects like a [NodeMbox], and [NodeState].
//!
//! ### Add zencan-build as build dependency
//!
//! This crate contains functions to generate the object dictionary code from the device config TOML
//! file.
//!
//! ```toml
//! [build-dependencies]
//! zencan-build = "0.0.1"
//! ```
//!
//! ### Add the code generation to your `build.rs` file
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
//! ### Include the generated code in your application
//!
//! When including the code, it is included using the name specified in build -- `ZENCAN_CONFIG` in
//! this case. This allows creating multiple object dictionaries in a single application.
//!
//! Typically, an application would add a snippet like this into `main.rs`:
//!
//! ```ignore
//! mod zencan {
//!     zencan_node::include_modules!(ZENCAN_CONFIG);
//! }
//! ```
//!
//! ## Instantiating the [`Node`] object
//!
//! ### Object setup
//!
//! One of the first things you should do before instantiating a node is set the serial number on
//! the 0x1018 object. Devices are identified by an "identity" that includes the vendor, product,
//! revision, and a serial number. If you are going to put more than one of a particular device type
//! on a network, each should somehow come up with a unique serial. Most MCUs will have a unique ID
//! register which can be used for this purpose.
//!
//! ```ignore
//! // Use the UID register to set a unique serial number
//! zencan::OBJECT1018.set_serial(get_serial());
//! ```
//!
//! ### Node Creation
//!
//!
//! Instantiate the node by providing it with the node ID, a set of event callbacks, the object
//! dictionary, the mailbox, and the node state object.
//!
//! - Node ID: This is the ID boot up ID of the node. It can be stored in flash, it can be a
//!   constant, it can be set by DIP switches, etc. It can also be left as `NodeId::Unconfigured`.
//!   It is then possible to configure the node ID over the bus using the LSS protocol.
//! - Object dictionary: This is a table where all of the objects are stored. It is created as a
//!   static variable by `zencan-build`, and is called `OD_TABLE`.
//! - Mailbox: This is a data structure for receiving incoming CAN messages. It buffers received
//!   messages so that messages can be pass to it in an interrupt, and then processed in the next
//!   call to `process`. It is defined by the generated code in a static variable named `NODE_MBOX`.
//! - Node state: This is a global state structure which provides some communications between the
//!   Node and objects such as PDO configuration objects, or special purpose object like the Save
//!   Command object. It is defined by the generated code in a static variable named `NODE_STATE`.
//!
//! There are a variety of callback functions you may provide as well, although they are not
//! required.
//!
//!
//! ```ignore
//! // Get references to the functions which save to flash
//! let store_node_config = &mut store_node_config;
//! let store_objects = &mut store_objects;
//!
//! let callbacks = Callbacks {
//!     store_node_config: Some(store_node_config),
//!     store_objects: Some(store_objects),
//!     reset_app: None,
//!     reset_comms: None,
//!     enter_operational: None,
//!     enter_stopped: None,
//!     enter_preoperational: None,
//! };
//!
//!
//! // Initialize node, providing references to the static objects created by `zencan-build`
//! let mut node = Node::new(
//!     NodeId::Unconfigured,
//!     callbacks,
//!     &zencan::NODE_MBOX,
//!     &zencan::NODE_STATE,
//!     &zencan::OD_TABLE,
//! );
//! ```
//!
//! ## Handling CAN messages
//!
//! The application has to handle sending and receiving CAN messages.
//!
//! The NODE_MBOX struct acts as a mailbox for both incoming and outgoing mailboxes, and the
//! application must pass messages between the mailbox and the CAN controller.  This can be done in
//! any thread -- a good way to do it is to have the CAN controller interrupt store messages here
//! directly.
//!
//! ```ignore
//! // Assuming we've received a message (id, and buffer) from somewhere, pass it to the mailbox
//! let msg = zencan_node::common::messages::CanMessage::new(id, &buffer[..msg.len as usize]);
//! // Ignore error -- as an Err is returned for messages that are not consumed by the node
//! // stack. You may handle those some other way, or simply drop them.
//! zencan::NODE_MBOX.store_message(msg).ok();
//! ```
//!
//! Outgoing messages can be read from the mbox using the [`NodeMbox::next_transmit_message`]
//! function. A callback can be registered (see [`NodeMbox::set_transmit_notify_callback`]) to be
//! notified when new messages are queued for transmission -- this can be used to e.g. push the
//! first message(s) to the CAN controller to initiate an IRQ driven transmit look, or to wake an
//! async task which is responsible for moving messages from the node to the CAN controller.
//!
//! ```ignore
//! #[embassy_executor::task]
//! async fn twai_tx_task(mut twai_tx: TwaiTx<'static, Async>) {
//!     loop {
//!         while let Some(msg) = zencan::NODE_MBOX.next_transmit_message() {
//!             let frame =
//!                 EspTwaiFrame::new(StandardId::new(msg.id.raw() as u16).unwrap(), msg.data())
//!                     .unwrap();
//!             if let Err(e) = twai_tx.transmit_async(&frame).await {
//!                 log::error!("Error sending CAN message: {e:?}");
//!             }
//!         }
//!
//!         // Wait for wakeup signal when new CAN messages become ready for sending
//!         CANOPEN_TX_SIGNAL.wait().await;
//!     }
//! }
//! ```
//!
//! ## Calling periodic process method
//!
//! To execute the Node logic, the [`Node::process`] function must be called periodically.  While it
//! is possible to call process only periodically, the NODE_MBOX object provides a
//! [callback](NodeMbox::set_process_notify_callback) which can be used to notify another task that
//! process should be called when a message is received and requires processing.
//!
//! Here's an example of a lilos task which executes process when either CAN_NOTIFY is signals, or
//! 10ms has passed since the last notification.
//!
//! ```ignore
//! async fn can_task(
//!     mut node: Node,
//! ) -> Infallible {
//!     let epoch = lilos::time::TickTime::now();
//!     loop {
//!         lilos::time::with_timeout(Duration::from_millis(10), CAN_NOTIFY.until_next()).await;
//!         let time_us = epoch.elapsed().0 * 1000;
//!         node.process(time_us);
//!     }
//! }
//! ```
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
pub mod priority_queue;
mod sdo_server;
pub mod storage;

// Re-export proc macros
pub use zencan_macro::build_object_dict;

// Re-export types used by generated code
pub use critical_section;
pub use zencan_common as common;

pub use bootloader::{BootloaderInfo, BootloaderSection, BootloaderSectionCallbacks};
#[cfg(feature = "socketcan")]
#[cfg_attr(docsrs, doc(cfg(feature = "socketcan")))]
pub use common::open_socketcan;
pub use node::{Callbacks, Node};
pub use node_mbox::NodeMbox;
pub use node_state::{NodeState, NodeStateAccess};
pub use persist::{restore_stored_comm_objects, restore_stored_objects};
pub use sdo_server::SDO_BUFFER_SIZE;

/// Include the code generated for the object dict in the build script.
#[macro_export]
macro_rules! include_modules {
    ($name: tt) => {
        include!(env!(concat!(
            "ZENCAN_INCLUDE_GENERATED_",
            stringify!($name),
        )));
    };
}
