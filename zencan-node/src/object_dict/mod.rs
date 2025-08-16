//! Object dictionary implementation
//!
//! The object dictionary is typically generated using `zencan-build`, using the types provided
//! here.
//!
//!
//! ## Topics
//!
//! ### PDO event triggering
//!

mod object_flags;
mod objects;
mod sub_objects;

// Pull up public sub module definitions. The submodules provide some code organization, but
// shouldn't clutter the public API
pub use object_flags::*;
pub use objects::*;
pub use sub_objects::*;
