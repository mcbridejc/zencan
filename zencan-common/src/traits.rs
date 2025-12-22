//! Common traits

use core::time::Duration;

use crate::messages::CanMessage;

/// A trait for accessing a value
///
/// E.g. from an AtomicCell
pub trait LoadStore<T> {
    /// Read the value
    fn load(&self) -> T;
    /// Store a new value to the
    fn store(&self, value: T);
}

/// A synchronous can sender
pub trait CanSender {
    /// Send a message to the bus
    fn send(&mut self, msg: CanMessage) -> Result<(), CanMessage>;
}

/// A synchronous can receiver
pub trait CanReceiver {
    /// The error type returned by recv
    type Error;
    /// Attempt to read a message from the receiver, and return None immediately if no message is
    /// available
    fn try_recv(&mut self) -> Option<CanMessage>;
    /// A blocking receive with timeout
    fn recv(&mut self, timeout: Duration) -> Result<CanMessage, Self::Error>;
}

/// An async CAN sender trait
pub trait AsyncCanSender: Send {
    /// Error type returned by sender
    type Error: CanSendError;
    /// Send a message to the bus
    fn send(
        &mut self,
        msg: CanMessage,
    ) -> impl core::future::Future<Output = Result<(), Self::Error>>;
}

/// A trait for CAN errors which may come from different types of interfaces
///
/// On no_std, all the error can do is return the unsent frame. With `std`, it can convert any
/// underlying errors into a String.
pub trait CanSendError: core::fmt::Debug {
    /// Convert the error into the undelivered message
    fn into_can_message(self) -> CanMessage;

    /// Get a string describing the error
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    fn message(&self) -> String;
}

/// An async CAN receiver trait
pub trait AsyncCanReceiver: Send {
    /// The error type returned by recv
    type Error: core::fmt::Debug + Send;

    /// Receive available message immediately
    fn try_recv(&mut self) -> Option<CanMessage>;

    /// A blocking receive
    fn recv(
        &mut self,
    ) -> impl core::future::Future<Output = Result<CanMessage, Self::Error>> + Send;

    /// Remove any pending messages from the receiver
    fn flush(&mut self) {
        while self.try_recv().is_some() {}
    }
}
