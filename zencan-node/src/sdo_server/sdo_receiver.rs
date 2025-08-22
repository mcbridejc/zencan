use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use zencan_common::{
    sdo::{BlockSegment, SdoRequest},
    AtomicCell,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReceiverState {
    Normal,
    BlockReceive,
    BlockCompleted {
        ackseq: u8,
        last_segment: u8,
        complete: bool,
    },
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
/// It includes a data buffer, as during block downloads, message data is written directly into
/// the buffer in the IRQ. Since no response message is required for block segments until they are
/// all received, they may come in faster than process is executed to handle them.
///
/// A timer is also reset to 0 on each message received, and this can be used in `process()` to
/// implement a timeout in case an expected message is never received.
pub(crate) struct SdoReceiver {
    request: AtomicCell<Option<SdoRequest>>,
    state: AtomicCell<ReceiverState>,
    buffer: AtomicCell<Option<&'static mut [u8]>>,
    timer: UnsafeCell<u32>,
    last_seqnum: UnsafeCell<u8>,
    blksize: UnsafeCell<u8>,
}

unsafe impl Sync for SdoReceiver {}

impl SdoReceiver {
    pub const fn new(sdo_buffer: &'static mut [u8]) -> Self {
        Self {
            request: AtomicCell::new(None),
            state: AtomicCell::new(ReceiverState::Normal),
            buffer: AtomicCell::new(Some(sdo_buffer)),
            timer: UnsafeCell::new(0),
            last_seqnum: UnsafeCell::new(0),
            blksize: UnsafeCell::new(0),
        }
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
                    critical_section::with(|_| unsafe {
                        *self.timer.get() = 0;
                    });
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
                critical_section::with(|_| unsafe {
                    *self.timer.get() = 0;
                    // seqnum comes from a 7-bit field so max possible value is 127
                    let pos = (segment.seqnum - 1) as usize * 7;
                    if pos + 7 <= buffer.len() {
                        buffer[pos..pos + 7].copy_from_slice(&segment.data);
                    }

                    if segment.seqnum == *self.last_seqnum.get() + 1 {
                        *self.last_seqnum.get() += 1;
                    }

                    if segment.seqnum == *self.blksize.get() || segment.c {
                        self.state.store(ReceiverState::BlockCompleted {
                            ackseq: *self.last_seqnum.get(),
                            last_segment: segment.seqnum,
                            complete: segment.c,
                        });
                        process_required = true;
                    }
                });
                process_required
            }
            // Block transfer has ended, and process should handle it
            ReceiverState::BlockCompleted { .. } => true,
        }
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
        critical_section::with(|_| unsafe {
            *self.last_seqnum.get() = 0;
            *self.timer.get() = 0;
            *self.blksize.get() = blksize;
            self.set_state(ReceiverState::BlockReceive);
        });
    }

    pub(crate) fn restart_block_download(&self, ackseq: u8) {
        critical_section::with(|_| unsafe {
            *self.last_seqnum.get() = ackseq;
            *self.timer.get() = 0;
            self.set_state(ReceiverState::BlockReceive);
        });
    }

    pub(crate) fn increment_timer(&self, elapsed_us: u32) -> u32 {
        let mut timer = 0;
        critical_section::with(|_| unsafe {
            *self.timer.get() = (*self.timer.get()).saturating_add(elapsed_us);
            timer = *self.timer.get();
        });
        timer
    }
}
