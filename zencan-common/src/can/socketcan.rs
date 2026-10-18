//! Utilities for opening a linux socketcan device
use std::sync::Arc;

use super::{
    AsyncCanReceiver, AsyncCanSender, CanError, CanFdMessage, CanFrame, CanId, CanMessage,
    CanSendError,
};
use snafu::{ResultExt, Snafu};
use socketcan::{
    frame::AsPtr, id::FdFlags, CanAnyFrame, CanFdFrame, CanFdSocket, CanSocket, EmbeddedFrame,
    Frame, ShouldRetry, Socket,
};
use tokio::io::{unix::AsyncFd, Interest};

fn socketcan_id_to_zencan_id(id: socketcan::CanId) -> CanId {
    match id {
        socketcan::CanId::Standard(id) => CanId::std(id.as_raw()),
        socketcan::CanId::Extended(id) => CanId::extended(id.as_raw()),
    }
}

fn zencan_id_to_socketcan_id(id: CanId) -> socketcan::CanId {
    match id {
        CanId::Extended(id) => socketcan::ExtendedId::new(id).unwrap().into(),
        CanId::Std(id) => socketcan::StandardId::new(id).unwrap().into(),
    }
}

fn socketcan_frame_to_zencan_message(frame: socketcan::CanFrame) -> Result<CanMessage, CanError> {
    let id = socketcan_id_to_zencan_id(frame.can_id());

    match frame {
        socketcan::CanFrame::Data(frame) => Ok(CanMessage::new(id, frame.data())),
        socketcan::CanFrame::Remote(_) => Ok(CanMessage::new_rtr(id)),
        socketcan::CanFrame::Error(frame) => Err(CanError::from_raw(frame.error_bits() as u8)),
    }
}

fn zencan_message_to_socket_frame(frame: CanMessage) -> socketcan::CanFrame {
    let id = zencan_id_to_socketcan_id(frame.id());

    if frame.is_rtr() {
        socketcan::CanFrame::new_remote(id, 0).unwrap()
    } else {
        socketcan::CanFrame::new(id, frame.data()).unwrap()
    }
}

/// Convert a frame read from a CAN FD socket
///
/// Classic data frames are widened to CAN FD frames without bit rate switching. Remote frames have
/// no CAN FD equivalent, and yield `None`.
fn socketcan_any_frame_to_zencan_fd_message(
    frame: CanAnyFrame,
) -> Result<Option<CanFdMessage>, CanError> {
    let id = socketcan_id_to_zencan_id(frame.can_id());

    match frame {
        CanAnyFrame::Normal(frame) => Ok(Some(CanFdMessage::new(id, frame.data()).with_brs(false))),
        CanAnyFrame::Fd(frame) => {
            let mut msg = CanFdMessage::new(id, frame.data()).with_brs(frame.is_brs());
            msg.set_esi(frame.is_esi());
            Ok(Some(msg))
        }
        CanAnyFrame::Remote(_) => Ok(None),
        CanAnyFrame::Error(frame) => Err(CanError::from_raw(frame.error_bits() as u8)),
    }
}

/// Convert a CAN FD message for transmission, padded to its on-wire length
///
/// The ESI flag is left to the controller, which sets it from its own error state.
fn zencan_fd_message_to_socket_frame(msg: &CanFdMessage) -> CanFdFrame {
    let flags = if msg.brs() {
        FdFlags::BRS
    } else {
        FdFlags::empty()
    };
    // Unwrap: a CanFdMessage never exceeds 64 bytes
    CanFdFrame::with_flags(
        zencan_id_to_socketcan_id(msg.id()),
        msg.padded_data(),
        flags,
    )
    .unwrap()
}

/// A handle to a socketcan CAN socket implementing AsyncCanReceiver.
#[derive(Debug, Clone)]
pub struct SocketCanReceiver {
    socket: Arc<AsyncCanSocket>,
}

#[derive(Debug, Snafu)]
pub enum ReceiveError {
    Io { source: socketcan::IoError },
    Can { source: CanError },
}

#[derive(Debug, Snafu)]
pub struct SendError<F: CanFrame = CanMessage> {
    source: socketcan::IoError,
    message: F,
}

impl<F: CanFrame> CanSendError<F> for SendError<F> {
    fn into_can_message(self) -> F {
        self.message
    }

    fn message(&self) -> String {
        self.source.to_string()
    }
}

/// Create an Async socket around a socketcan socket. This is just a reimplemenation of the tokio
/// socket in the `socketcan` crate, but with support for `try_read_frame` and `try_write_frame`
/// added.
#[derive(Debug)]
struct AsyncCanSocket<S: Socket = CanSocket>(AsyncFd<S>);

#[allow(dead_code)]
impl<S: Socket> AsyncCanSocket<S> {
    pub fn new(inner: S) -> Result<Self, std::io::Error> {
        inner.set_nonblocking(true)?;
        // SAFETY: The socket owns its file descriptor, which stays open and unchanged until the
        // socket is dropped together with the AsyncFd.
        Ok(Self(unsafe { AsyncFd::register(inner) }?))
    }

    pub fn open(ifname: &str) -> Result<Self, std::io::Error> {
        Self::new(S::open(ifname)?)
    }

    /// Attempt to read a CAN frame from the socket without blocking
    ///
    /// If no message is immediately available, a WouldBlock error is returned.
    pub fn try_read_frame(&self) -> Result<S::FrameType, std::io::Error> {
        self.0.get_ref().read_frame()
    }

    /// Read a CAN frame from the socket asynchronously
    pub async fn read_frame(&self) -> Result<S::FrameType, std::io::Error> {
        self.0
            .async_io(Interest::READABLE, |inner| inner.read_frame())
            .await
    }

    pub async fn write_frame<F>(&self, frame: &F) -> Result<(), std::io::Error>
    where
        F: Into<S::FrameType> + AsPtr,
    {
        self.0
            .async_io(Interest::WRITABLE, |inner| inner.write_frame(frame))
            .await
    }

    /// Attempt to write a CAN frame to the socket without blocking
    pub fn try_write_frame<F>(&self, frame: F) -> Result<(), std::io::Error>
    where
        F: Into<S::FrameType> + AsPtr,
    {
        self.0.get_ref().write_frame(&frame)
    }
}

impl AsyncCanReceiver for SocketCanReceiver {
    type Error = ReceiveError;

    fn try_recv(&mut self) -> Option<CanMessage> {
        match self.socket.try_read_frame() {
            Ok(frame) => Some(socketcan_frame_to_zencan_message(frame).unwrap()),
            _ => None,
        }
    }

    async fn recv(&mut self) -> Result<CanMessage, ReceiveError> {
        loop {
            match self.socket.read_frame().await {
                Ok(frame) => return socketcan_frame_to_zencan_message(frame).context(CanSnafu),
                Err(e) => {
                    if !e.should_retry() {
                        return Err(ReceiveError::Io { source: e });
                    }
                }
            }
        }
    }
}

/// A handle to a socketcan CAN socket implementing AsyncCanSender.
#[derive(Debug, Clone)]
pub struct SocketCanSender {
    socket: Arc<AsyncCanSocket>,
}

impl AsyncCanSender for SocketCanSender {
    type Error = SendError;
    async fn send(&mut self, msg: CanMessage) -> Result<(), Self::Error> {
        let socketcan_frame = zencan_message_to_socket_frame(msg);

        self.socket
            .write_frame(&socketcan_frame)
            .await
            .context(SendSnafu { message: msg })
    }
}

/// Open a socketcan device and split it into a sender and receiver object for use with zencan
/// library
///
/// # Arguments
/// * `device` - The name of the socketcan device to open, e.g. "vcan0", or "can0"
///
/// A key benefit of this is that by creating both sender and receiver objects from a shared socket,
/// the receiver will not receive messages sent by the sender.
#[cfg_attr(docsrs, doc(cfg(feature = "socketcan")))]
pub fn open_socketcan<S: AsRef<str>>(
    device: S,
) -> Result<(SocketCanSender, SocketCanReceiver), socketcan::IoError> {
    let device: &str = device.as_ref();
    let socket = Arc::new(AsyncCanSocket::open(device)?);
    let receiver = SocketCanReceiver {
        socket: socket.clone(),
    };
    let sender = SocketCanSender { socket };
    Ok((sender, receiver))
}

/// A handle to a CAN FD socketcan socket implementing `AsyncCanReceiver<CanFdMessage>`
#[derive(Debug, Clone)]
pub struct SocketCanFdReceiver {
    socket: Arc<AsyncCanSocket<CanFdSocket>>,
}

impl AsyncCanReceiver<CanFdMessage> for SocketCanFdReceiver {
    type Error = ReceiveError;

    fn try_recv(&mut self) -> Option<CanFdMessage> {
        // Skip the frames which cannot be represented: remote frames and error frames
        while let Ok(frame) = self.socket.try_read_frame() {
            if let Ok(Some(msg)) = socketcan_any_frame_to_zencan_fd_message(frame) {
                return Some(msg);
            }
        }
        None
    }

    async fn recv(&mut self) -> Result<CanFdMessage, ReceiveError> {
        loop {
            match self.socket.read_frame().await {
                Ok(frame) => {
                    // Remote frames have no CAN FD equivalent: wait for the next frame
                    if let Some(msg) =
                        socketcan_any_frame_to_zencan_fd_message(frame).context(CanSnafu)?
                    {
                        return Ok(msg);
                    }
                }
                Err(e) => {
                    if !e.should_retry() {
                        return Err(ReceiveError::Io { source: e });
                    }
                }
            }
        }
    }
}

/// A handle to a CAN FD socketcan socket implementing `AsyncCanSender<CanFdMessage>`
#[derive(Debug, Clone)]
pub struct SocketCanFdSender {
    socket: Arc<AsyncCanSocket<CanFdSocket>>,
}

impl AsyncCanSender<CanFdMessage> for SocketCanFdSender {
    type Error = SendError<CanFdMessage>;
    async fn send(&mut self, msg: CanFdMessage) -> Result<(), Self::Error> {
        let socketcan_frame = zencan_fd_message_to_socket_frame(&msg);

        self.socket
            .write_frame(&socketcan_frame)
            .await
            .context(SendSnafu { message: msg })
    }
}

/// Fail early when `device` cannot carry CAN FD frames, rather than on the first transmission
///
/// The MTU is queried over netlink. When it cannot be read, the check is skipped and transmission
/// errors will report the problem instead.
fn check_fd_mtu(device: &str) -> Result<(), socketcan::IoError> {
    let mtu = socketcan::CanInterface::open(device)
        .ok()
        .and_then(|interface| interface.details().ok())
        .and_then(|details| details.mtu);
    if mtu == Some(socketcan::nl::Mtu::Standard) {
        return Err(socketcan::IoError::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "{device} is not configured for CAN FD (MTU {}, CAN FD requires {}); enable it \
                 with `ip link set {device} type can bitrate <rate> dbitrate <rate> fd on`, or \
                 `ip link set {device} mtu {}` for a vcan device",
                socketcan::nl::Mtu::Standard as u32,
                socketcan::nl::Mtu::Fd as u32,
                socketcan::nl::Mtu::Fd as u32,
            ),
        ));
    }
    Ok(())
}

/// Open a socketcan device for CAN FD and split it into a sender and receiver object for use with
/// zencan library
///
/// The socket is opened with `CAN_RAW_FD_FRAMES` enabled. It transmits [`CanFdMessage`] frames,
/// padded to a legal CAN FD length and with bit rate switching as requested by each frame. It
/// receives both CAN FD frames and classic data frames, the latter widened to [`CanFdMessage`];
/// remote frames, which do not exist in CAN FD, are discarded.
///
/// To run classic CAN code, such as the zencan-client SDO client, over the returned handles, wrap
/// them in a [`ClassicAdapter`](super::ClassicAdapter).
///
/// # Arguments
/// * `device` - The name of the socketcan device to open, e.g. "vcan0", or "can0". It must be
///   configured for CAN FD (MTU 72), otherwise an `InvalidInput` error is returned.
#[cfg_attr(docsrs, doc(cfg(feature = "socketcan")))]
pub fn open_socketcan_fd<S: AsRef<str>>(
    device: S,
) -> Result<(SocketCanFdSender, SocketCanFdReceiver), socketcan::IoError> {
    let device: &str = device.as_ref();
    check_fd_mtu(device)?;
    let socket = Arc::new(AsyncCanSocket::<CanFdSocket>::open(device)?);
    let receiver = SocketCanFdReceiver {
        socket: socket.clone(),
    };
    let sender = SocketCanFdSender { socket };
    Ok((sender, receiver))
}

#[cfg(test)]
mod tests {
    use super::*;
    use socketcan::{CanDataFrame, CanRemoteFrame};

    #[test]
    fn test_fd_message_is_padded_for_transmission() {
        let msg = CanFdMessage::new(CanId::Std(0x181), &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let frame = zencan_fd_message_to_socket_frame(&msg);
        assert_eq!(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0], frame.data());
        assert!(frame.is_brs());
        assert!(!frame.is_esi());
        assert!(frame.flags().contains(FdFlags::FDF));
        assert_eq!(0x181, frame.raw_id());

        let msg = CanFdMessage::new(CanId::Extended(0x1234567), &[0xAA; 64]).with_brs(false);
        let frame = zencan_fd_message_to_socket_frame(&msg);
        assert_eq!(&[0xAA; 64], frame.data());
        assert!(!frame.is_brs());
        assert!(frame.is_extended());
    }

    #[test]
    fn test_fd_frame_is_received() {
        let id = socketcan::StandardId::new(0x281).unwrap();
        let mut frame = CanFdFrame::with_flags(id, &[7; 20], FdFlags::BRS).unwrap();
        frame.set_esi(true);
        let msg = socketcan_any_frame_to_zencan_fd_message(frame.into())
            .unwrap()
            .unwrap();
        assert_eq!(CanId::Std(0x281), msg.id());
        assert_eq!(&[7; 20], msg.data());
        assert!(msg.brs());
        assert!(msg.esi());
    }

    #[test]
    fn test_classic_frames_on_fd_socket() {
        let id = socketcan::StandardId::new(0x000).unwrap();
        let data = CanDataFrame::new(id, &[1, 5]).unwrap();
        let msg = socketcan_any_frame_to_zencan_fd_message(CanAnyFrame::Normal(data))
            .unwrap()
            .unwrap();
        assert_eq!(CanId::Std(0), msg.id());
        assert_eq!(&[1, 5], msg.data());
        assert!(!msg.brs());

        let remote = CanRemoteFrame::new_remote(id, 0).unwrap();
        assert_eq!(
            None,
            socketcan_any_frame_to_zencan_fd_message(CanAnyFrame::Remote(remote)).unwrap()
        );
    }
}
