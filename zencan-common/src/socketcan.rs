use std::sync::Arc;

use crate::{
    messages::{CanError, CanId, CanMessage},
    traits::{AsyncCanReceiver, AsyncCanSender},
};
use snafu::{ResultExt, Snafu};
use socketcan::{CanFrame, CanSocket, EmbeddedFrame, Frame, ShouldRetry, Socket};
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
        CanFrame::Data(frame) => Ok(CanMessage::new(id, frame.data())),
        CanFrame::Remote(_) => Ok(CanMessage::new_rtr(id)),
        CanFrame::Error(frame) => Err(CanError::from_raw(frame.error_bits() as u8)),
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

#[derive(Debug, Clone)]
pub struct SocketCanReceiver {
    socket: Arc<AsyncCanSocket>,
}

#[derive(Debug, Snafu)]
pub enum ReceiveError {
    Io { source: socketcan::IoError },
    Can { source: CanError },
}

/// Create an Async socket around a socketcan CanSocket. This is just a reimplemenation of the tokio
/// socket in the `socketcan` crate, but with support for `try_read_frame` and `try_write_frame`
/// added.
#[derive(Debug)]
struct AsyncCanSocket(AsyncFd<CanSocket>);

#[allow(dead_code)]
impl AsyncCanSocket {
    pub fn new(inner: CanSocket) -> Result<Self, std::io::Error> {
        inner.set_nonblocking(true)?;
        Ok(Self(AsyncFd::new(inner)?))
    }

    pub fn open(ifname: &str) -> Result<Self, std::io::Error> {
        let socket = CanSocket::open(ifname)?;
        socket.set_nonblocking(true)?;
        Ok(Self(AsyncFd::new(socket)?))
    }

    /// Attempt to read a CAN frame from the socket without blocking
    ///
    /// If no message is immediately available, a WouldBlock error is returned.
    pub fn try_read_frame(&self) -> Result<CanFrame, std::io::Error> {
        self.0.get_ref().read_frame()
    }

    /// Read a CAN frame from the socket asynchronously
    pub async fn read_frame(&self) -> Result<CanFrame, std::io::Error> {
        self.0
            .async_io(Interest::READABLE, |inner| inner.read_frame())
            .await
    }

    pub async fn write_frame(&self, frame: &CanFrame) -> Result<(), std::io::Error> {
        self.0
            .async_io(Interest::WRITABLE, |inner| inner.write_frame(frame))
            .await
    }

    /// Attempt to write a CAN frame to the socket without blocking
    pub fn try_write_frame(&self, frame: CanFrame) -> Result<(), std::io::Error> {
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

#[derive(Debug, Clone)]
pub struct SocketCanSender {
    socket: Arc<AsyncCanSocket>,
}

impl AsyncCanSender for SocketCanSender {
    async fn send(&mut self, msg: CanMessage) -> Result<(), CanMessage> {
        let socketcan_frame = zencan_message_to_socket_frame(msg);

        let result = self.socket.write_frame(&socketcan_frame).await;
        if result.is_err() {
            Err(msg)
        } else {
            Ok(())
        }
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
