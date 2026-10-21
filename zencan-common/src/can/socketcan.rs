//! Utilities for opening a linux socketcan device
use std::sync::Arc;

use super::{AsyncCanReceiver, AsyncCanSender, CanError, CanId, CanMessage, CanSendError};
use snafu::{ResultExt, Snafu};
use socketcan::{tokio::CanSocket, CanFrame, EmbeddedFrame, Frame, ShouldRetry};

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

/// A handle to a socketcan CAN socket implementing AsyncCanReceiver.
#[derive(Debug, Clone)]
pub struct SocketCanReceiver {
    socket: Arc<CanSocket>,
}

#[derive(Debug, Snafu)]
pub enum ReceiveError {
    Io { source: socketcan::IoError },
    Can { source: CanError },
}

#[derive(Debug, Snafu)]
pub struct SendError {
    source: socketcan::IoError,
    message: CanMessage,
}

impl CanSendError for SendError {
    fn into_can_message(self) -> CanMessage {
        self.message
    }

    fn message(&self) -> String {
        self.source.to_string()
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
    socket: Arc<CanSocket>,
}

impl AsyncCanSender for SocketCanSender {
    type Error = SendError;
    async fn send(&mut self, msg: CanMessage) -> Result<(), Self::Error> {
        let socketcan_frame = zencan_message_to_socket_frame(msg);

        self.socket
            .write_frame(socketcan_frame)
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
    let socket = Arc::new(CanSocket::open(device)?);
    let receiver = SocketCanReceiver {
        socket: socket.clone(),
    };
    let sender = SocketCanSender { socket };
    Ok((sender, receiver))
}
