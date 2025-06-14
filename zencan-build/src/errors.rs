//! Error types for crate
//!
use snafu::Snafu;

/// Error returned when loading a device config
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
#[allow(missing_docs)]
pub enum CompileError {
    /// General error with context
    #[snafu(display("General error '{message}'. source: {source}"))]
    General {
        message: String,
        source: Box<dyn std::error::Error>,
    },
    InvalidFieldName {
        field_name: String,
    },
    ParseInt {
        message: String,
        source: std::num::ParseIntError,
    },
    ParseFloat {
        message: String,
        source: std::num::ParseFloatError,
    },
    DefaultValueTooLong {
        message: String,
    },
    DefaultValueTypeMismatch {
        message: String,
    },
    NotRunViaCargo,
    Io {
        source: std::io::Error,
    },
    DeviceConfig {
        source: zencan_common::device_config::LoadError,
    },
}
