//! Transport traits for sending and receiving CAN frames
//!
//! Each trait is generic over the frame type it carries, with [`CanMessage`] (classic CAN) as the
//! default. A transport implements the traits for the one frame format it speaks: classic
//! transports implement e.g. `AsyncCanSender`, CAN FD transports implement
//! `AsyncCanSender<CanFdMessage>`.
//!
//! [`CanFdMessage`]: super::CanFdMessage
use core::time::Duration;

use super::{CanFrame, CanMessage};

/// A synchronous can sender
pub trait CanSender<F: CanFrame = CanMessage> {
    /// Send a message to the bus
    fn send(&mut self, msg: F) -> Result<(), F>;
}

/// A synchronous can receiver
pub trait CanReceiver<F: CanFrame = CanMessage> {
    /// The error type returned by recv
    type Error;
    /// Attempt to read a message from the receiver, and return None immediately if no message is
    /// available
    fn try_recv(&mut self) -> Option<F>;
    /// A blocking receive with timeout
    fn recv(&mut self, timeout: Duration) -> Result<F, Self::Error>;
}

/// An async CAN sender trait
pub trait AsyncCanSender<F: CanFrame = CanMessage>: Send {
    /// Error type returned by sender
    type Error: CanSendError<F>;
    /// Send a message to the bus
    fn send(&mut self, msg: F) -> impl core::future::Future<Output = Result<(), Self::Error>>;
}

/// A trait for CAN errors which may come from different types of interfaces
///
/// On no_std, all the error can do is return the unsent frame. With `std`, it can convert any
/// underlying errors into a String.
pub trait CanSendError<F: CanFrame = CanMessage>: core::fmt::Debug {
    /// Convert the error into the undelivered message
    fn into_can_message(self) -> F;

    /// Get a string describing the error
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    fn message(&self) -> String;
}

/// An async CAN receiver trait
pub trait AsyncCanReceiver<F: CanFrame = CanMessage>: Send {
    /// The error type returned by recv
    type Error: core::fmt::Debug + Send;

    /// Receive available message immediately
    fn try_recv(&mut self) -> Option<F>;

    /// A blocking receive
    fn recv(&mut self) -> impl core::future::Future<Output = Result<F, Self::Error>> + Send;

    /// Remove any pending messages from the receiver
    fn flush(&mut self) {
        while self.try_recv().is_some() {}
    }
}
