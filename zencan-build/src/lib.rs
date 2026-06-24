//! Tools for generating a zencan node
//!
//! ## Device Config File
//!
//! A "device config" is a TOML file, which defines the behavior of a node. It has some general
//! configuration options, like how many RPDOs/TPDOs the device should support, and it creates the
//! set of application specific objects that will be accessible in the node's object dictionary. It
//! is the input used by `zencan-build` to generate code for the node.
//!
//! The file is read using [`DeviceConfig`].
//!
//! ## Generating code using build.rs
//!
//! The expected way to use this crate is in your project's `build.rs` file. The
//! [`build_node_from_device_config()`] function can be used there to generate the code from a
//! device config file, and store it under a provided label. Then, in your code, the
//! `include_modules!` macro from the `zencan-node` create can be used to include the code wherever
//! you want.
//!
//! ### Example
//!
//! In build.rs:
//!
//! ```ignore
//! if let Err(e) =
//!     zencan_build::build_node_from_device_config("EXAMPLE", "example_device_config.toml")
//! {
//!     eprintln!("Error building node from example_device_config.toml: {}", e);
//!     std::process::exit(1);
//! }
//! ```
//!
//! Then, in main.rs:
//!
//! ```ignore
//! mod zencan {
//!     zencan_node::include_modules!(EXAMPLE);
//! }
//! ```
//!
//! ## The generated code
//!
//! Generated storage is private and initialized on the first `get_od()` call.
//! Keep the returned reference to access objects without repeating the guard:
//!
//! ```ignore
//! let od = zencan::get_od();
//! od.object1018().set_serial(get_serial());
//! let node = zencan_node::Node::new(node_id, callbacks, od.node_mbox(), od.node_state(), od);
//! ```
//!
//! The dictionary owns its objects and runtime state in one static allocation,
//! with one initialization guard. Its `repr(C)` construction view wraps each
//! field in `MaybeUninit`; compile-time assertions verify both views have the
//! same layout. Fields are constructed in place before the dictionary is
//! published, so references between fields remain stable. The ready check is
//! inline; first-time construction lives in a separate non-inlined function.
//! Borrow owned fields when passing them to APIs: `od.node_state()`,
//! `od.node_mbox()`, and `od.od_table()`. Default reset changes values through
//! interior mutability without replacing objects or their references.
//! NMT reset calls the generated reset dispatcher before application callbacks.
//!
#![warn(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations
)]

use std::path::Path;

use snafu::ResultExt;

mod codegen;
pub mod device_config;
mod elaboration;
pub mod errors;
mod object_build_spec;
mod system_objects;

pub use codegen::device_config_to_string;
//pub use codegen::device_config_to_tokens;
use device_config::DeviceConfig;

use errors::*;

/// Compile a device config TOML file into rust code
///
/// # Arguments
///
/// * `config_path` - Path to the device config TOML file
/// * `out_path` - Path to write the generated code to
pub fn compile_device_config(
    config_path: impl AsRef<Path>,
    out_path: impl AsRef<Path>,
) -> Result<(), CompileError> {
    let config = DeviceConfig::load(config_path.as_ref()).context(DeviceConfigSnafu)?;

    let code = device_config_to_string(&config, true)?.to_string();

    std::fs::write(out_path.as_ref(), code.as_bytes()).context(IoSnafu)?;
    Ok(())
}

/// Generate a node for inclusion via `include_modules!` macro
///
/// This is intended to be run in build.rs.
///
/// The provided name is used to allow for multiple object dictionaries to be generated in a single
/// project. It is simply a way to reference a particular generated code file from the
/// `include_modules!` macro.
///
/// # Example
///
/// ```ignore
/// if let Err(e) =
///     zencan_build::build_node_from_device_config("EXAMPLE", "example_device_config.toml")
/// {
///     eprintln!("Error building node from example_device_config.toml: {}", e);
///     std::process::exit(1);
/// }
/// ```
pub fn build_node_from_device_config(
    name: &str,
    config_path: impl AsRef<Path>,
) -> Result<(), CompileError> {
    let output_file_path =
        Path::new(&std::env::var_os("OUT_DIR").ok_or(NotRunViaCargoSnafu.build())?)
            .join(format!("zencan_node_{}.rs", name));

    compile_device_config(&config_path, &output_file_path)?;

    let env_var = format!("ZENCAN_INCLUDE_GENERATED_{}", name);
    println!("cargo:rustc-env={}={}", env_var, output_file_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertables::assert_contains;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_error_display() {
        // Object with missing index
        let mut input_file = NamedTempFile::new().expect("Failed to create tempfile");
        input_file
            .write_all(
                r#"
            device_name = "test"
            [[objects]]
        "#
                .as_bytes(),
            )
            .expect("Failed writing input file");
        let out_file = NamedTempFile::new().expect("Failed to create tempfile");
        let err = compile_device_config(input_file.path(), out_file.path());
        assert!(err.is_err());
        assert_contains!(err.unwrap_err().to_string(), "missing field `index`");

        // Incorrect default value
        let mut input_file = NamedTempFile::new().expect("Failed to create tempfile");
        input_file
            .write_all(
                r#"
            device_name = "test"

            [identity]
            vendor_id = 1
            product_code = 2
            revision_number = 3

            [[objects]]
            index = 0x1
            object_type = "var"
            access_type = "rw"
            data_type = "VisibleString(16)"
            default_value = 0
        "#
                .as_bytes(),
            )
            .expect("Failed writing input file");
        let out_file = NamedTempFile::new().expect("Failed to create tempfile");
        let err = compile_device_config(input_file.path(), out_file.path());
        assert!(err.is_err());
        assert_contains!(err.unwrap_err().to_string(), "DefaultValueTypeMismatch: Default integer value 0 is not a valid value for type VisibleString(16)");
    }
}
