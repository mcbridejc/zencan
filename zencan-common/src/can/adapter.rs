//! Run classic CAN code over a CAN FD transport
use super::{AsyncCanReceiver, AsyncCanSender, CanFdMessage, CanMessage, CanSendError, FrameError};

/// Presents a CAN FD transport as a classic CAN transport
///
/// Wrapping an `AsyncCanSender<CanFdMessage>` or an `AsyncCanReceiver<CanFdMessage>` provides the
/// corresponding classic `AsyncCanSender` or `AsyncCanReceiver`, so that code written for classic
/// CAN frames, such as the zencan-client SDO client, can run on a CAN FD bus:
///
/// - Classic frames are sent as CAN FD frames with the same payload, with bit rate switching
///   requested. Remote frames have no CAN FD equivalent and are rejected.
/// - Received frames of up to 8 bytes are delivered as classic frames. Longer frames have no
///   classic equivalent and are skipped.
///
/// This works because every message of the CANopen protocols zencan implements, except PDOs, fits
/// in 8 bytes.
#[derive(Clone, Copy, Debug)]
pub struct ClassicAdapter<T> {
    inner: T,
}

impl<T> ClassicAdapter<T> {
    /// Wrap a CAN FD transport
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Get a reference to the wrapped transport
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Unwrap the wrapped transport
    pub fn into_inner(self) -> T {
        self.inner
    }
}

/// Error returned when sending through a [`ClassicAdapter`]
#[derive(Debug)]
pub enum ClassicAdapterSendError<E> {
    /// The frame has no CAN FD equivalent (a remote frame), and was not sent
    Unsupported {
        /// The frame which was not sent
        message: CanMessage,
        /// The reason the frame could not be converted
        reason: FrameError,
    },
    /// The wrapped transport failed to send the frame
    Transport(E),
}

impl<E: CanSendError<CanFdMessage>> CanSendError for ClassicAdapterSendError<E> {
    fn into_can_message(self) -> CanMessage {
        match self {
            ClassicAdapterSendError::Unsupported { message, .. } => message,
            // Unwrap: the frame was widened from a classic frame, so it narrows back
            ClassicAdapterSendError::Transport(e) => {
                CanMessage::try_from(e.into_can_message()).unwrap()
            }
        }
    }

    #[cfg(feature = "std")]
    fn message(&self) -> String {
        match self {
            ClassicAdapterSendError::Unsupported { reason, .. } => reason.to_string(),
            ClassicAdapterSendError::Transport(e) => e.message(),
        }
    }
}

impl<T: AsyncCanSender<CanFdMessage>> AsyncCanSender for ClassicAdapter<T> {
    type Error = ClassicAdapterSendError<T::Error>;

    async fn send(&mut self, msg: CanMessage) -> Result<(), Self::Error> {
        let frame =
            CanFdMessage::try_from(msg).map_err(|reason| ClassicAdapterSendError::Unsupported {
                message: msg,
                reason,
            })?;
        self.inner
            .send(frame)
            .await
            .map_err(ClassicAdapterSendError::Transport)
    }
}

impl<T: AsyncCanReceiver<CanFdMessage>> AsyncCanReceiver for ClassicAdapter<T> {
    type Error = T::Error;

    fn try_recv(&mut self) -> Option<CanMessage> {
        while let Some(frame) = self.inner.try_recv() {
            if let Ok(msg) = CanMessage::try_from(frame) {
                return Some(msg);
            }
        }
        None
    }

    async fn recv(&mut self) -> Result<CanMessage, Self::Error> {
        loop {
            if let Ok(msg) = CanMessage::try_from(self.inner.recv().await?) {
                return Ok(msg);
            }
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use std::collections::VecDeque;
    use std::future::Future;

    use super::*;
    use crate::can::CanId;

    /// Run a future which never waits
    fn run<T>(future: impl Future<Output = T>) -> T {
        let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
        match core::pin::pin!(future).poll(&mut cx) {
            core::task::Poll::Ready(value) => value,
            core::task::Poll::Pending => panic!("future is not ready"),
        }
    }

    #[derive(Default)]
    struct FdBus {
        frames: VecDeque<CanFdMessage>,
    }

    #[derive(Debug)]
    struct NeverFails;

    impl CanSendError<CanFdMessage> for NeverFails {
        fn into_can_message(self) -> CanFdMessage {
            unreachable!()
        }

        fn message(&self) -> String {
            unreachable!()
        }
    }

    impl AsyncCanSender<CanFdMessage> for FdBus {
        type Error = NeverFails;

        async fn send(&mut self, msg: CanFdMessage) -> Result<(), NeverFails> {
            self.frames.push_back(msg);
            Ok(())
        }
    }

    impl AsyncCanReceiver<CanFdMessage> for FdBus {
        type Error = ();

        fn try_recv(&mut self) -> Option<CanFdMessage> {
            self.frames.pop_front()
        }

        async fn recv(&mut self) -> Result<CanFdMessage, ()> {
            self.frames.pop_front().ok_or(())
        }
    }

    #[test]
    fn test_classic_frames_are_sent_as_fd_frames() {
        let mut adapter = ClassicAdapter::new(FdBus::default());
        let msg = CanMessage::new(CanId::Std(0x601), &[0x40, 0x00, 0x10, 0, 0, 0, 0, 0]);
        run(adapter.send(msg)).unwrap();

        let sent = adapter.inner().frames[0];
        assert_eq!(msg.id(), sent.id());
        assert_eq!(msg.data(), sent.data());
        assert!(sent.brs());
    }

    #[test]
    fn test_remote_frames_are_not_sent() {
        let mut adapter = ClassicAdapter::new(FdBus::default());
        let rtr = CanMessage::new_rtr(CanId::Std(0x701));
        let err = run(adapter.send(rtr)).unwrap_err();
        assert!(matches!(
            err,
            ClassicAdapterSendError::Unsupported {
                reason: FrameError::RtrNotSupported,
                ..
            }
        ));
        assert_eq!(rtr, err.into_can_message());
        assert!(adapter.into_inner().frames.is_empty());
    }

    #[test]
    fn test_long_fd_frames_are_skipped() {
        let mut bus = FdBus::default();
        bus.frames
            .push_back(CanFdMessage::new(CanId::Std(0x181), &[1; 64]));
        bus.frames
            .push_back(CanFdMessage::new(CanId::Std(0x581), &[2; 8]));
        bus.frames
            .push_back(CanFdMessage::new(CanId::Std(0x281), &[3; 12]));
        bus.frames
            .push_back(CanFdMessage::new(CanId::Std(0x701), &[5]));
        let mut adapter = ClassicAdapter::new(bus);

        assert_eq!(
            Some(CanMessage::new(CanId::Std(0x581), &[2; 8])),
            adapter.try_recv()
        );
        assert_eq!(
            Ok(CanMessage::new(CanId::Std(0x701), &[5])),
            run(adapter.recv())
        );
        assert_eq!(None, adapter.try_recv());
    }
}
