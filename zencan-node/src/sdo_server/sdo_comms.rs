use core::{
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
};

use portable_atomic::{AtomicU32, AtomicU8};
use zencan_common::{
    sdo::{BlockSegment, SdoRequest, SdoResponse},
    AtomicCell,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReceiverState {
    Normal,
    BlockReceive,
    BlockReceiveCompleted {
        ackseq: u8,
        last_segment: u8,
        complete: bool,
    },
    BlockSend {
        block_size: u32,
        current_segment: u8,
        send_complete: bool,
    },
    BlockSendCompleted,
    BlockSendAborted,
}

pub struct BufferGuard<'a> {
    buf: Option<&'static mut [u8]>,
    home: &'a AtomicCell<Option<&'static mut [u8]>>,
}

impl Drop for BufferGuard<'_> {
    fn drop(&mut self) {
        self.home.store(Some(self.buf.take().unwrap()));
    }
}

impl Deref for BufferGuard<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.buf.as_ref().unwrap()
    }
}

impl DerefMut for BufferGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buf.as_mut().unwrap()
    }
}

/// Data structure for communicating SDO data between receiving and processing threads
///
/// It includes a data buffer, as during block transfers, message data is read/written directly
/// from/to the buffer in the receive thread. Since no response message is required for block
/// segments until they are all received, they may come in faster than process is executed to handle
/// them.
///
/// A timer is also reset to 0 on each message received, and this can be used in `process()` to
/// implement a timeout in case an expected message is never received.
pub(crate) struct SdoComms {
    request: AtomicCell<Option<SdoRequest>>,
    response: AtomicCell<Option<SdoResponse>>,
    state: AtomicCell<ReceiverState>,
    buffer: AtomicCell<Option<&'static mut [u8]>>,
    timer: AtomicU32,
    last_seqnum: AtomicU8,
    blksize: AtomicU8,
}

impl SdoComms {
    pub const fn new(sdo_buffer: &'static mut [u8]) -> Self {
        Self {
            request: AtomicCell::new(None),
            response: AtomicCell::new(None),
            state: AtomicCell::new(ReceiverState::Normal),
            buffer: AtomicCell::new(Some(sdo_buffer)),
            timer: AtomicU32::new(0),
            last_seqnum: AtomicU8::new(0),
            blksize: AtomicU8::new(0),
        }
    }

    pub fn next_transmit_message(&self) -> Option<[u8; 8]> {
        // Always send a queued response if avaliable
        if let Some(resp) = self.response.take().map(|resp| resp.to_bytes()) {
            return Some(resp);
        }
        critical_section::with(|_| {
            match self.state.load() {
                ReceiverState::BlockSend {
                    block_size,
                    current_segment,
                    send_complete,
                } => {
                    // Send blocks
                    let total_segments = (block_size as usize).div_ceil(7);
                    let read_idx = current_segment as usize * 7;
                    let bytes_remaining = block_size as usize - read_idx;
                    let segment_size = bytes_remaining.min(7);
                    let last_segment_in_subblock = current_segment == total_segments as u8 - 1;
                    let c = send_complete && last_segment_in_subblock;
                    let mut data = [0; 7];
                    data[..segment_size].copy_from_slice(
                        &self.borrow_buffer()[current_segment as usize * 7
                            ..current_segment as usize * 7 + segment_size],
                    );
                    let msg = BlockSegment {
                        c,
                        seqnum: current_segment + 1,
                        data,
                    };

                    if last_segment_in_subblock {
                        self.state.store(ReceiverState::BlockSendCompleted);
                    } else {
                        self.state.store(ReceiverState::BlockSend {
                            block_size,
                            current_segment: current_segment + 1,
                            send_complete,
                        });
                    }
                    Some(msg.to_bytes())
                }
                _ => None,
            }
        })
    }

    /// Handle received request from client
    pub fn handle_req(&self, msg_data: &[u8]) -> bool {
        // Ignore invalid lengths
        if msg_data.len() != 8 {
            return false;
        }

        match self.state.load() {
            ReceiverState::Normal => match msg_data.try_into() {
                Ok(req) => {
                    self.request.store(Some(req));
                    self.timer.store(0, Ordering::Relaxed);
                    true
                }
                Err(_) => false,
            },
            ReceiverState::BlockReceive => {
                // In block receive state, we expect that all messages are blocks, but also check
                // for abort messages. Abort messages can be distinguished from blocks because a
                // segment seqnum of 0 is not allowed, and abort messages have 0x80 in the first
                // byte, which would correspond to seqnum = 0 if it was a block segment.
                if msg_data[0] == 0x80 {
                    if let Ok(req) = SdoRequest::try_from(msg_data) {
                        self.request.store(Some(req));
                        self.set_state(ReceiverState::Normal);
                        return true;
                    }
                }

                // Unwrap: Can only fail when len is != 8, and that is checked above
                let segment = BlockSegment::try_from(msg_data).unwrap();
                if segment.seqnum == 0 {
                    // seqnum 0 isn't allowed. Ignore it.
                    return false;
                }

                let mut buffer = self.borrow_buffer();

                let mut process_required = false;
                critical_section::with(|_| {
                    self.timer.store(0, Ordering::Relaxed);
                    // seqnum comes from a 7-bit field so max possible value is 127
                    let pos = (segment.seqnum - 1) as usize * 7;
                    if pos + 7 <= buffer.len() {
                        buffer[pos..pos + 7].copy_from_slice(&segment.data);
                    }

                    if segment.seqnum == self.last_seqnum.load(Ordering::Relaxed) + 1 {
                        self.last_seqnum.fetch_add(1, Ordering::Relaxed);
                    }

                    if segment.seqnum == self.blksize.load(Ordering::Relaxed) || segment.c {
                        self.state.store(ReceiverState::BlockReceiveCompleted {
                            ackseq: self.last_seqnum.load(Ordering::Relaxed),
                            last_segment: segment.seqnum,
                            complete: segment.c,
                        });
                        process_required = true;
                    }
                });
                process_required
            }
            ReceiverState::BlockReceiveCompleted { .. } => true,
            ReceiverState::BlockSend {
                block_size: _,
                send_complete: _,
                current_segment: _,
            } => {
                // In block send state, we don't expect any requests except for possible abort
                // messages. Abort messages can be distinguished from blocks because a
                // segment seqnum of 0 is not allowed, and abort messages have 0x80 in the first
                // byte, which would correspond to seqnum = 0 if it was a block segment.
                if msg_data[0] == 0x80 {
                    self.state.store(ReceiverState::BlockSendAborted);
                    true
                } else {
                    false
                }
            }
            ReceiverState::BlockSendCompleted => {
                if let Ok(req) = msg_data.try_into() {
                    self.request.store(Some(req));
                    self.timer.store(0, Ordering::Relaxed);
                }
                true
            }
            ReceiverState::BlockSendAborted => {
                if let Ok(req) = msg_data.try_into() {
                    self.request.store(Some(req));
                    self.timer.store(0, Ordering::Relaxed);
                }
                true
            }
        }
    }

    pub(crate) fn store_response(&self, resp: SdoResponse) {
        self.response.store(Some(resp));
    }

    pub(crate) fn set_state(&self, state: ReceiverState) {
        self.state.store(state);
    }

    pub(crate) fn state(&self) -> ReceiverState {
        self.state.load()
    }

    /// Borrow the SDO buffer from the receiver
    ///
    /// It will be returned on drop.
    ///
    /// This function will panic if the buffer has already been borrowed, or if the buffer was never
    /// set via `store_buffer`.
    pub(crate) fn borrow_buffer(&self) -> BufferGuard<'_> {
        let buf = self.buffer.take();

        BufferGuard {
            buf,
            home: &self.buffer,
        }
    }

    pub(crate) fn take_request(&self) -> Option<SdoRequest> {
        self.request.take()
    }

    pub(crate) fn begin_block_download(&self, blksize: u8) {
        critical_section::with(|_| {
            self.last_seqnum.store(0, Ordering::Relaxed);
            self.timer.store(0, Ordering::Relaxed);
            self.blksize.store(blksize, Ordering::Relaxed);
            self.set_state(ReceiverState::BlockReceive);
        });
    }

    pub(crate) fn restart_block_download(&self, ackseq: u8) {
        critical_section::with(|_| {
            self.last_seqnum.store(ackseq, Ordering::Relaxed);
            self.timer.store(0, Ordering::Relaxed);
            self.set_state(ReceiverState::BlockReceive);
        });
    }

    pub(crate) fn begin_block_upload(&self, size: usize, send_complete: bool) {
        critical_section::with(|_| {
            self.timer.store(0, Ordering::Relaxed);
            self.set_state(ReceiverState::BlockSend {
                block_size: size as u32,
                current_segment: 0,
                send_complete,
            });
        })
    }

    pub(crate) fn increment_timer(&self, elapsed_us: u32) -> u32 {
        self.timer.add(elapsed_us, Ordering::Relaxed);
        self.timer.load(Ordering::Relaxed)
    }
}
