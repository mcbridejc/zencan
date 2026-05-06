//! Common traits

use core::time::Duration;

use arbitrary_int::{i24, u24};

use crate::messages::CanMessage;

/// A trait for computing the read size of some types
pub trait ReadSize {
    /// Read size of the type
    const READ_SIZE: usize;
}

macro_rules! impl_read_size_builtin {
    ($($rust_type:ty),+ $(,)?) => {
        $(
            impl ReadSize for $rust_type {
                const READ_SIZE: usize = core::mem::size_of::<$rust_type>();
            }
        )+
    };
}
impl_read_size_builtin!(bool, u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

macro_rules! impl_read_size_arbitrary {
    ($($rust_type:ty),+ $(,)?) => {
        $(
            impl ReadSize for $rust_type {
                const READ_SIZE: usize = <$rust_type>::BITS.div_ceil(8);
            }
        )+
    };
}
impl_read_size_arbitrary!(u24, i24);

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
