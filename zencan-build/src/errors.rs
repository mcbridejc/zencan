//! Error types for crate
//!
use snafu::Snafu;
use zencan_common::object_model::ObjectSpec;

/// Error returned when loading a device config
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
#[allow(missing_docs)]
pub enum CompileError {
    #[snafu(display("Found duplicate object for index 0x{:x}: ({}/{})", object1.index, object1.parameter_name, object2.parameter_name))]
    DuplicateObjectIndex {
        object1: Box<ObjectSpec>,
        object2: Box<ObjectSpec>,
    },
    /// Provided field name is not a valid rust ident
    #[snafu(display("InvalidFieldName: {field_name} is not a valid rust ident"))]
    InvalidFieldName { field_name: String },
    /// Default value is too long for the container size
    #[snafu(display("DefaultValueTooLong: {message}"))]
    DefaultValueTooLong { message: String },
    /// Default value does not match the object type
    #[snafu(display("DefaultValueTypeMismatch: {message}"))]
    DefaultValueTypeMismatch { message: String },
    /// Invalid PDO default configuration.
    #[snafu(display("Invalid PDO defaults: {message}"))]
    InvalidPdoDefaults { message: String },
    /// Missing cargo env vars
    #[snafu(display("NotRunViaCargo: Missing expected cargo env variables"))]
    NotRunViaCargo,
    /// An IO error occurred while writing generated code
    #[snafu(display("Io: {source}"))]
    Io { source: std::io::Error },
    /// An error occurred while loading the device config file
    #[snafu(display("Error loading device config: {source}"))]
    DeviceConfig {
        source: crate::device_config::LoadError,
    },
}
