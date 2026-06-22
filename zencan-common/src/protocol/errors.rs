use crate::can::CanId;
use snafu::Snafu;

/// An error for problems converting CanMessages to zencan types
#[derive(Debug, Clone, Copy, PartialEq, Snafu)]
pub enum MessageError {
    /// Not enough bytes were present in the message
    MessageTooShort,
    /// The message was malformed in some way
    MalformedMsg {
        /// The COB ID of the malformed message
        cob_id: CanId,
    },
    /// The message ID was not the expected value
    #[snafu(display("Unexpected message ID found: {cob_id:?}, expected: {expected:?}"))]
    UnexpectedId {
        /// Received ID
        cob_id: CanId,
        /// Expected ID
        expected: CanId,
    },
    /// A field in the message contained an unallowed value for that field
    InvalidField,
    /// The COB ID of the message does not correspond to an expected ZencanMessag
    ///
    /// This isn't particular surprising, many messages on the bus will not (e.g. PDOs)
    UnrecognizedId {
        /// The unrecognized COB
        cob_id: CanId,
    },
    /// The NMT state integer in the message is not a valid NMT state
    InvalidNmtState {
        /// The invalid byte
        value: u8,
    },
    /// An invalid LSS command specifier was found in the message
    #[snafu(display("Unexpected LSS command: {value}"))]
    UnexpectedLssCommand {
        /// The invalid byte
        value: u8,
    },
}
