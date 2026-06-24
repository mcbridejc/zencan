//! Object Dictionary
//!
//! # Objects Overview
//!
//! The object dictionary is the main mechanism of configuration and communication for a node. For
//! example, SDO access is performed on sub objects, which are identified by the 16-bit object ID of
//! their parent object, and an 8-bit sub index. Objects come in three varieties:
//!
//! - VAR: A single variable of any type (accessed at sub index 0)
//! - ARRAY: An array of sub-objects, all with the same type. Sub-index 0 is a u8 containing the
//!   size of the array. Sub indices 1-N contain the array values.
//! - RECORD: A collection of sub-objects of heterogenous types. Sub-index 0 contains the highest
//!   implemented sub index.
//!
//! The set of data types which are be stored are defined by the [`DataType`](crate::common::object_model::DataType) enum.
//!
//! The object dictionary is generated at build time using the `zencan-build` crate, based on the
//! device config TOML file. A goal of zencan is to minimize the amount of generated code, so the
//! generated code primarily instantiates the types defined here.
//!
//! # Object Storage
//!
//! Most objects implement their own storage, and are statically allocated. However, it is possible
//! to register object handlers at run-time, which may store data in any way they wish, and perform
//! whatever logic is required upon access to the object. The objects must be declared as
//! `application_callback` objects at build time, so that a [`CallbackObject`] is inserted into the
//! object dictionary as a placeholder to store the run-time provided object.
//!
//! # The ObjectAccess trait
//!
//! Any struct which implements the [`ObjectAccess`] trait can be used to represent an object in the
//! dictionary. For simple data objects, the object can be defined in TOML and a type implementing
//! this trait will be created for it during code generation. Additionally, accessor methods will be
//! defined for accessing the sub objects directly.
//!
//! For more complex logic, custom objects can be implemented by implementing the [`ObjectAccess`]
//! trait. A more ergonomic way to implement this trait is to implement the [`ProvidesSubObjects`]
//! trait, and implement the sub objects individually by implementing the [`SubObjectAccess`] trait.
//! Any object which implements [`ProvidesSubObjects`] will also get an [`ObjectAccess`]
//! implementation.
//!
//! ## SubObject implementations
//!
//! Most sub objects can be implemented using one of the following existing types:
//!
//! - [`ScalarFieldU32`], etc
//! - [`ByteField``]
//! - [`NullTermByteField`]
//! - [`ConstField`]
//! - [`ConstByteRefField`]
//!
//! ## Example Custom Object Implementation
//!
//! ```rust
//! use zencan_node::object_dict::{ConstField, ScalarFieldU32, ProvidesSubObjects, SubInfo, SubObjectAccess};
//! use zencan_node::common::object_model::ObjectCode;
//! use zencan_node::common::protocol::AbortCode;
//! // Example external API used to access a value for a sub field
//! struct ExternalApi {}
//!
//! impl ExternalApi {
//!     pub fn get_value(&self) -> f32 {
//!         42.0
//!     }
//!     pub fn set_value(&self, value: f32) {
//!         // TODO: Do something with the value
//!     }
//! }
//!
//! struct ExternalSubObject {
//!     external_api: &'static ExternalApi,
//! }
//!
//! impl ExternalSubObject {
//!     pub fn new(external_api: &'static ExternalApi) -> Self {
//!         Self { external_api }
//!     }
//! }
//!
//! impl SubObjectAccess for ExternalSubObject {
//!     fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
//!         let value_bytes = self.external_api.get_value().to_le_bytes();
//!         if offset < value_bytes.len() {
//!             let read_len = buf.len().min(value_bytes.len() - offset);
//!             buf[..read_len].copy_from_slice(&value_bytes[offset..offset + read_len]);
//!             Ok(read_len)
//!         } else {
//!             Ok(0)
//!         }
//!     }
//!
//!     fn read_size(&self) -> usize {
//!         4
//!     }
//!
//!     fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
//!         if data.len() == 4 {
//!             let value = f32::from_le_bytes(data.try_into().unwrap());
//!             self.external_api.set_value(value);
//!             Ok(())
//!         } else if data.len() < 4 {
//!             Err(AbortCode::DataTypeMismatchLengthLow)
//!         } else if data.len() > 4 {
//!             Err(AbortCode::DataTypeMismatchLengthHigh)
//!         } else {
//!             let value = f32::from_le_bytes(data.try_into().unwrap());
//!             self.external_api.set_value(value);
//!             Ok(())
//!         }
//!     }
//! }
//!
//! struct CustomObject {
//!     stored_field: ScalarFieldU32,
//!     external_field: ExternalSubObject,
//! }
//!
//! impl CustomObject {
//!     pub fn new(external_api: &'static ExternalApi) -> Self {
//!         Self {
//!             external_field: ExternalSubObject::new(external_api),
//!             stored_field: ScalarFieldU32::new(0),
//!         }
//!     }
//! }
//!
//! impl ProvidesSubObjects for CustomObject {
//!     fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
//!         match sub {
//!             // Sub 0 returns the highest sub index on the object
//!             0 => Some((
//!                 SubInfo::MAX_SUB_NUMBER,
//!                 const { &ConstField::new(3u8.to_le_bytes()) },
//!             )),
//!             // Sub 1 returns the u32 field stored in the object, implemented using ScalarFieldU32
//!             1 => Some((SubInfo::new_u32().rw_access().persist(true), &self.stored_field)),
//!             // Sub 2 returns a custom sub object which accesses the external API
//!             2 => Some((SubInfo::new_f32().rw_access().persist(false), &self.external_field)),
//!             _ => None,
//!         }
//!     }
//!
//!     fn object_code(&self) -> ObjectCode {
//!         ObjectCode::Record
//!     }
//! }
//! ```
//!
//! # Object threading support
//!
//! All object must be `Sync` and `Send`, to allow for access from any thread. This is implemented
//! using the `critical_section` crate. All objects support [`ObjectAccess::read`] and
//! [`ObjectAccess::write`], which allow for atomic access of objects. For small objects, the SDO
//! server will access objects using a single read or write call, buffering the data for segmented
//! or block transfers, ensuring atomic access. However, if the size of the transfer is larger than
//! the SDO buffer (currently fixed at 889 bytes, but likely to become adjustable in the future)
//! then the SDO server is unable to buffer all of the data. For reading data, this will result in
//! multiple calls to `read` with no guarantees that the data will not change in between, so it is
//! possible that a client can get a "torn read". For writing data to an object, the partial write
//! API is used, and has similar concerns.
//!
//! # Object flags for TPDO event triggering
//!
//! Some objects support event flags, which can be set via [`ObjectAccess::set_event_flag`]. These
//! are used to trigger TPDO transmission.
//!

mod object_flags;
mod objects;
mod sub_info;
mod sub_objects;

// Pull up public sub module definitions. The submodules provide some code organization, but
// shouldn't clutter the public API
pub use object_flags::*;
pub use objects::*;
pub use sub_info::*;
pub use sub_objects::*;
