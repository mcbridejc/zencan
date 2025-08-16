use crc16::CrcType as _;
use zencan_common::{
    objects::{DataType, ObjectId, SubInfo},
    sdo::{AbortCode, SdoRequest, SdoResponse},
};

use crate::object_dict::{find_object_entry, ODEntry};

use crate::sdo_server::{sdo_receiver::ReceiverState, SdoReceiver};

/// Size of block transfers Always support max of 127 segments in block transfers. This may have to
/// adjust to support configurable buffer size
const BLKSIZE: u8 = 127;

/// Number of microseconds to wait for a message before timing out an SDO transaction
const SDO_TIMEOUT_US: u32 = 25000;

fn validate_download_size(dl_size: usize, subobj: &SubInfo) -> Result<(), AbortCode> {
    if subobj.size == 0 {
        // Some objects (e.g. domains) do not provide a size, and we simply must write to them and
        // see if it fails. These objects report a size of 0.
        return Ok(());
    }
    if subobj.data_type.is_str() || matches!(subobj.data_type, DataType::Domain) {
        // Strings can write shorter lengths
        if dl_size > subobj.size {
            return Err(AbortCode::DataTypeMismatchLengthHigh);
        }
    } else {
        // All other types require exact size
        if dl_size < subobj.size {
            return Err(AbortCode::DataTypeMismatchLengthLow);
        } else if dl_size > subobj.size {
            return Err(AbortCode::DataTypeMismatchLengthHigh);
        }
    }
    Ok(())
}

struct SdoResult {
    response: Option<SdoResponse>,
    updated_object: Option<ObjectId>,
    new_state: SdoState,
}

impl SdoResult {
    fn no_response(new_state: SdoState) -> Self {
        Self {
            response: None,
            updated_object: None,
            new_state,
        }
    }

    fn abort(index: u16, sub: u8, abort_code: AbortCode) -> Self {
        Self {
            response: Some(SdoResponse::abort(index, sub, abort_code)),
            updated_object: None,
            new_state: SdoState::Idle,
        }
    }

    fn response(response: SdoResponse, new_state: SdoState) -> Self {
        Self {
            response: Some(response),
            updated_object: None,
            new_state,
        }
    }

    fn response_with_update(
        response: SdoResponse,
        index: u16,
        sub: u8,
        new_state: SdoState,
    ) -> Self {
        Self {
            response: Some(response),
            updated_object: Some(ObjectId { index, sub }),
            new_state,
        }
    }
}

#[derive(Clone, Copy)]
struct Segmented {
    object: &'static ODEntry<'static>,
    sub: u8,
    toggle_state: bool,
    segment_counter: u32,
    bytes_in_buffer: Option<u32>,
}

#[derive(Clone, Copy)]
struct DownloadBlock {
    sub: u8,
    last_segment: u8,
    client_supports_crc: bool,
    crc: u16,
    block_counter: usize,
    object: &'static ODEntry<'static>,
}

enum SdoState {
    Idle,
    DownloadSegmented(Segmented),
    UploadSegmented(Segmented),
    DownloadBlock(DownloadBlock),
    EndDownloadBlock(DownloadBlock),
}

impl SdoState {
    pub fn update(
        &self,
        rx: &SdoReceiver,
        elapsed_us: u32,
        od: &'static [ODEntry<'static>],
    ) -> SdoResult {
        match self {
            SdoState::Idle => Self::idle(od, rx),
            SdoState::DownloadSegmented(state) => Self::download_segmented(state, rx, elapsed_us),
            SdoState::UploadSegmented(state) => Self::upload_segmented(state, rx, elapsed_us),
            SdoState::DownloadBlock(state) => Self::download_block(state, rx, elapsed_us),
            SdoState::EndDownloadBlock(state) => Self::end_download_block(state, rx, elapsed_us),
        }
    }

    fn idle(od: &'static [ODEntry<'static>], rx: &SdoReceiver) -> SdoResult {
        let req = match rx.take_request() {
            Some(req) => req,
            None => return SdoResult::no_response(SdoState::Idle),
        };

        match req {
            SdoRequest::InitiateDownload {
                n,
                e,
                s,
                index,
                sub,
                data,
            } => {
                let od_entry = match find_object_entry(od, index) {
                    Some(x) => x,
                    None => return SdoResult::abort(index, sub, AbortCode::NoSuchObject),
                };
                let obj = &od_entry.data;

                let subinfo = match obj.sub_info(sub) {
                    Ok(s) => s,
                    Err(abort_code) => return SdoResult::abort(index, sub, abort_code),
                };

                if e {
                    // Doing an expedited download
                    // Verify that the requested object is writable
                    if !subinfo.access_type.is_writable() {
                        return SdoResult::abort(index, sub, AbortCode::ReadOnly);
                    }

                    // Verify data size requested by client fits object, and abort if not
                    let dl_size = 4 - n as usize;
                    if let Err(abort_code) = validate_download_size(dl_size, &subinfo) {
                        return SdoResult::abort(index, sub, abort_code);
                    }

                    if let Err(abort_code) = obj.write(sub, &data[0..dl_size]) {
                        return SdoResult::abort(index, sub, abort_code);
                    }

                    SdoResult {
                        response: Some(SdoResponse::download_acknowledge(index, sub)),
                        updated_object: Some(ObjectId { index, sub }),
                        new_state: SdoState::Idle,
                    }
                } else {
                    // starting a segmented download
                    // If size is provided, verify data size requested by client fits object, and
                    // abort if not
                    if s {
                        let dl_size = u32::from_le_bytes(data) as usize;
                        if let Err(abort_code) = validate_download_size(dl_size, &subinfo) {
                            return SdoResult::abort(index, sub, abort_code);
                        }
                    }

                    let new_state = SdoState::DownloadSegmented(Segmented {
                        object: od_entry,
                        sub,
                        toggle_state: false,
                        segment_counter: 0,
                        bytes_in_buffer: Some(0),
                    });
                    SdoResult::response(SdoResponse::download_acknowledge(index, sub), new_state)
                }
            }
            SdoRequest::InitiateUpload { index, sub } => {
                let od_entry = match find_object_entry(od, index) {
                    Some(x) => x,
                    None => return SdoResult::abort(index, sub, AbortCode::NoSuchObject),
                };
                let obj = od_entry.data;

                let mut full_buf = rx.borrow_buffer();
                let len = full_buf.len();
                // Limit buffer to be a multiple of segment size
                let buf = &mut full_buf[0..len - (len % 7)];
                let read_size = match obj.read(sub, 0, buf) {
                    Ok(s) => s,
                    Err(abort_code) => return SdoResult::abort(index, sub, abort_code),
                };

                let response = if read_size <= 4 {
                    // Do expedited upload
                    SdoResult::response(
                        SdoResponse::expedited_upload(index, sub, &buf[..read_size]),
                        SdoState::Idle,
                    )
                } else {
                    // Start a segmented upload

                    // If read size is less than the buffer length then the read is atomic and we
                    // can safely report the size of the read up front. If it is equal, then the
                    // read may be longer and we do not report the size up front because it may
                    // change in the interim. We can't achieve atomic reads for sub objects that are
                    // larger than the buffer, and so the read size may change. If large objects are
                    // written during an SDO transfer, it is possible for the client to receive a
                    // torn read, which is some combination of multiple values.
                    let ack_size = if read_size == buf.len() {
                        None
                    } else {
                        Some(read_size as u32)
                    };
                    SdoResult::response(
                        SdoResponse::upload_acknowledge(index, sub, ack_size),
                        SdoState::UploadSegmented(Segmented {
                            object: od_entry,
                            sub,
                            toggle_state: false,
                            segment_counter: 0,
                            bytes_in_buffer: ack_size,
                        }),
                    )
                };
                response
            }
            SdoRequest::InitiateBlockDownload {
                cc,
                s,
                index,
                sub,
                size,
            } => {
                // starting a block download
                let od_entry = match find_object_entry(od, index) {
                    Some(x) => x,
                    None => return SdoResult::abort(index, sub, AbortCode::NoSuchObject),
                };

                let subinfo = match od_entry.data.sub_info(sub) {
                    Ok(s) => s,
                    Err(abort_code) => return SdoResult::abort(index, sub, abort_code),
                };

                // If size is provided, verify data size requested by client fits object, and
                // abort if not
                if s {
                    if let Err(abort_code) = validate_download_size(size as usize, &subinfo) {
                        return SdoResult::abort(index, sub, abort_code);
                    }
                }

                rx.begin_block_download(BLKSIZE);
                SdoResult::response(
                    SdoResponse::block_download_acknowledge(true, index, sub, BLKSIZE),
                    SdoState::DownloadBlock(DownloadBlock {
                        object: od_entry,
                        sub,
                        client_supports_crc: cc,
                        block_counter: 0,
                        last_segment: 0,
                        crc: crc16::XMODEM::init(),
                    }),
                )
            }
            SdoRequest::InitiateBlockUpload {
                index,
                sub,
                blksize: _,
                pst: _,
            } => SdoResult::abort(index, sub, AbortCode::InvalidCommandSpecifier),

            _ => SdoResult::abort(0, 0, AbortCode::InvalidCommandSpecifier),
        }
    }

    fn download_segmented(state: &Segmented, rx: &SdoReceiver, elapsed_us: u32) -> SdoResult {
        let req = match rx.take_request() {
            Some(req) => req,
            None => {
                let time = rx.increment_timer(elapsed_us);
                if time > SDO_TIMEOUT_US {
                    return SdoResult::abort(state.object.index, state.sub, AbortCode::SdoTimeout);
                } else {
                    return SdoResult::no_response(SdoState::DownloadSegmented(*state));
                }
            }
        };

        match req {
            SdoRequest::DownloadSegment { t, n, c, data } => {
                if t != state.toggle_state {
                    return SdoResult::abort(
                        state.object.index,
                        state.sub,
                        AbortCode::ToggleNotAlternated,
                    );
                }

                let obj = &state.object.data;
                let mut buf = rx.borrow_buffer();

                // Offset into the objec
                let total_offset = state.segment_counter as usize * 7;
                // Offset into the current buffer
                let buffer_offset = total_offset % buf.len();

                let on_first_buffer = total_offset == buffer_offset;

                let segment_size = 7 - n as usize;

                let copy_len = segment_size.min(buf.len() - buffer_offset);
                buf[buffer_offset..buffer_offset + copy_len].copy_from_slice(&data[0..copy_len]);

                let buffer_full = buffer_offset + copy_len == buf.len();
                let more_bytes_in_message = copy_len < segment_size;

                // See if we need to make this a partial write
                if buffer_full && (!c || more_bytes_in_message) {
                    if on_first_buffer {
                        if let Err(abort_code) = obj.begin_partial(state.sub) {
                            return SdoResult::abort(state.object.index, state.sub, abort_code);
                        }
                    }

                    if let Err(abort_code) = obj.write_partial(state.sub, &buf) {
                        return SdoResult::abort(state.object.index, state.sub, abort_code);
                    }

                    if more_bytes_in_message {
                        buf[0..segment_size - copy_len]
                            .copy_from_slice(&data[copy_len..segment_size]);
                    }
                }

                if c {
                    // See if we already opened a partial write
                    if (buffer_full && more_bytes_in_message) || !on_first_buffer {
                        if more_bytes_in_message {
                            if let Err(abort_code) =
                                obj.write_partial(state.sub, &buf[0..segment_size - copy_len])
                            {
                                return SdoResult::abort(state.object.index, state.sub, abort_code);
                            }
                        } else {
                            if let Err(abort_code) =
                                obj.write_partial(state.sub, &buf[..buffer_offset + segment_size])
                            {
                                return SdoResult::abort(state.object.index, state.sub, abort_code);
                            }
                        }
                        if let Err(abort_code) = obj.end_partial(state.sub) {
                            return SdoResult::abort(state.object.index, state.sub, abort_code);
                        }
                    } else {
                        if let Err(abort_code) =
                            obj.write(state.sub, &buf[0..buffer_offset + segment_size])
                        {
                            return SdoResult::abort(state.object.index, state.sub, abort_code);
                        }
                    }

                    SdoResult::response_with_update(
                        SdoResponse::download_segment_acknowledge(state.toggle_state),
                        state.object.index,
                        state.sub,
                        SdoState::Idle,
                    )
                } else {
                    // Segments that didn't fit in the buffer get stored to beginning of new buffer
                    if copy_len < segment_size {
                        buf[0..segment_size - copy_len]
                            .copy_from_slice(&data[copy_len..segment_size]);
                    }
                    // More segments remaining to be received
                    let new_state = SdoState::DownloadSegmented(Segmented {
                        toggle_state: !state.toggle_state,
                        segment_counter: state.segment_counter + 1,
                        ..*state
                    });
                    SdoResult::response(
                        SdoResponse::download_segment_acknowledge(state.toggle_state),
                        new_state,
                    )
                }
            }
            SdoRequest::Abort {
                index: _,
                sub: _,
                abort_code: _,
            } => SdoResult::no_response(SdoState::Idle),
            _ => SdoResult::abort(
                state.object.index,
                state.sub,
                AbortCode::InvalidCommandSpecifier,
            ),
        }
    }

    fn upload_segmented(state: &Segmented, rx: &SdoReceiver, elapsed_us: u32) -> SdoResult {
        let req = match rx.take_request() {
            Some(req) => req,
            None => {
                let time = rx.increment_timer(elapsed_us);
                if time > SDO_TIMEOUT_US {
                    return SdoResult::abort(state.object.index, state.sub, AbortCode::SdoTimeout);
                } else {
                    return SdoResult::no_response(SdoState::DownloadSegmented(*state));
                }
            }
        };
        match req {
            SdoRequest::ReqUploadSegment { t } => {
                if t != state.toggle_state {
                    return SdoResult::abort(
                        state.object.index,
                        state.sub,
                        AbortCode::ToggleNotAlternated,
                    );
                }

                let mut full_buf = rx.borrow_buffer();
                let len = full_buf.len();
                // Limit buffer to be a multiple of segment size
                let buf = &mut full_buf[0..len - (len % 7)];

                // How far into the object data we are
                let total_read_offset = state.segment_counter as usize * 7;
                // How far into the current buffer we are
                let buf_read_offset = total_read_offset % buf.len();

                let segment_size = if let Some(bytes_in_buffer) = state.bytes_in_buffer {
                    bytes_in_buffer as usize - buf_read_offset
                } else {
                    buf.len() - buf_read_offset
                }
                .min(7);
                let mut msg_buf = [0; 7];
                msg_buf[..segment_size]
                    .copy_from_slice(&buf[buf_read_offset..buf_read_offset + segment_size]);

                let mut c = false;
                let mut bytes_in_buffer = state.bytes_in_buffer;
                if state.bytes_in_buffer.is_none() {
                    if buf_read_offset + segment_size == buf.len() {
                        // We completed the buffered data. Read again to see if there is more data
                        // to send
                        let read_size = state
                            .object
                            .data
                            .read(state.sub, total_read_offset + segment_size, buf)
                            .unwrap();
                        if read_size == 0 {
                            // No further data in object, this is the last segment
                            c = true;
                        } else {
                            // We read more data. If the buffer was not filled, this is the last of
                            // it.
                            if read_size != buf.len() {
                                bytes_in_buffer = Some(read_size as u32)
                            }
                        }
                    }
                } else {
                    // This segment finished the bytes in this buffer
                    if buf_read_offset + segment_size == bytes_in_buffer.unwrap() as usize {
                        c = true;
                    }
                }

                let new_state = if c {
                    SdoState::Idle
                } else {
                    SdoState::UploadSegmented(Segmented {
                        object: state.object,
                        sub: state.sub,
                        toggle_state: !state.toggle_state,
                        segment_counter: state.segment_counter + 1,
                        bytes_in_buffer,
                    })
                };

                let response =
                    SdoResponse::upload_segment(state.toggle_state, c, &msg_buf[0..segment_size]);

                if c {
                    SdoResult::response_with_update(
                        response,
                        state.object.index,
                        state.sub,
                        new_state,
                    )
                } else {
                    SdoResult::response(response, new_state)
                }
            }
            SdoRequest::Abort {
                index: _,
                sub: _,
                abort_code: _,
            } => SdoResult::no_response(SdoState::Idle),
            _ => SdoResult::abort(
                state.object.index,
                state.sub,
                AbortCode::InvalidCommandSpecifier,
            ),
        }
    }

    fn download_block(state: &DownloadBlock, rx: &SdoReceiver, elapsed_us: u32) -> SdoResult {
        // During block download, up to 127 block segments are sent out in rapid succession, without
        // any acknowledgement, so the processing of these is handled in the receiver. Here, we wait
        // for the receiver to signal the completion of a block
        match rx.state() {
            // If receiver went back to normal state, that means it detected an Abort request
            ReceiverState::Normal => {
                // Remove the request from the mailbox
                let _ = rx.take_request();
                SdoResult::no_response(SdoState::Idle)
            }
            ReceiverState::BlockReceive => {
                // Still waiting. Check timeout.
                let time = rx.increment_timer(elapsed_us);
                if time > SDO_TIMEOUT_US {
                    rx.set_state(ReceiverState::Normal);
                    SdoResult::abort(state.object.index, state.sub, AbortCode::SdoTimeout)
                } else {
                    SdoResult::no_response(SdoState::DownloadBlock(*state))
                }
            }
            ReceiverState::BlockCompleted {
                ackseq,
                last_segment,
                complete,
            } => {
                // Some segment was missed, ask for retransmission
                // TODO: Request only the segments after ackseq instead of all of them
                if ackseq != last_segment {
                    rx.restart_block_download(ackseq);
                    SdoResult::response(
                        SdoResponse::ConfirmBlock {
                            ackseq,
                            blksize: BLKSIZE,
                        },
                        SdoState::DownloadBlock(*state),
                    )
                } else {
                    let new_state = if complete {
                        // This is the last block, but we can't do anything with it until we get the
                        // end block transfer request because we don't know how many bytes are
                        // invalid on the last segment
                        rx.set_state(ReceiverState::Normal);
                        SdoState::EndDownloadBlock(DownloadBlock {
                            block_counter: state.block_counter + 1,
                            last_segment,
                            ..*state
                        })
                    } else {
                        // Store the data from this block
                        let write_length = last_segment as usize * 7;

                        let buf = rx.borrow_buffer();
                        let valid_data = &buf[..write_length];

                        // Update the running CRC
                        let crc = crc16::XMODEM::update(state.crc, valid_data);

                        // If this is the first block of a multi-part block transfer, we begin
                        // partial write now. Not all objects support partial write, although
                        // generally any object large enough to warrant a multi-block transfer
                        // probably should.
                        if state.block_counter == 0 {
                            if let Err(abort_code) = state.object.data.begin_partial(state.sub) {
                                rx.set_state(ReceiverState::Normal);
                                return SdoResult::abort(state.object.index, state.sub, abort_code);
                            }
                        }

                        // Attempt to write the block. It may fail if, for example, the data exceeds
                        // the size of the object
                        if let Err(abort_code) =
                            state.object.data.write_partial(state.sub, valid_data)
                        {
                            rx.set_state(ReceiverState::Normal);
                            return SdoResult::abort(state.object.index, state.sub, abort_code);
                        }

                        // Prepare to download a new block
                        rx.begin_block_download(BLKSIZE);
                        SdoState::DownloadBlock(DownloadBlock {
                            block_counter: state.block_counter + 1,
                            crc,
                            ..*state
                        })
                    };
                    SdoResult::response(
                        SdoResponse::ConfirmBlock {
                            ackseq,
                            blksize: BLKSIZE,
                        },
                        new_state,
                    )
                }
            }
        }
    }

    fn end_download_block(state: &DownloadBlock, rx: &SdoReceiver, elapsed_us: u32) -> SdoResult {
        let req = match rx.take_request() {
            Some(req) => req,
            None => {
                let time = rx.increment_timer(elapsed_us);
                if time > SDO_TIMEOUT_US {
                    return SdoResult::abort(state.object.index, state.sub, AbortCode::SdoTimeout);
                } else {
                    return SdoResult::no_response(SdoState::EndDownloadBlock(*state));
                }
            }
        };

        match req {
            SdoRequest::EndBlockDownload { n, crc } => {
                let buf = rx.borrow_buffer();
                // Safety: If SDO protocol is followed, client cannot be sending
                // segments after the last segment, so no segments should be received
                // while we hold this shared ref and therefore no mut refs should exist

                let write_len = state.last_segment as usize * 7 - n as usize;
                let valid_data = &buf[..write_len];
                // Update the running CRC and check it
                let crc_calc = crc16::XMODEM::get(crc16::XMODEM::update(state.crc, valid_data));
                if state.client_supports_crc && crc_calc != crc {
                    return SdoResult::abort(state.object.index, state.sub, AbortCode::CrcError);
                }

                let objdata = &state.object.data;

                // Store the data from this block
                if state.block_counter == 1 {
                    // We only received a single block, so no partial transfer is required
                    if let Err(abort_code) = objdata.write(state.sub, valid_data) {
                        return SdoResult::abort(state.object.index, state.sub, abort_code);
                    }
                } else {
                    // This is the last block of a multi block transfer write it, and finish
                    if let Err(abort_code) = objdata.write_partial(state.sub, valid_data) {
                        return SdoResult::abort(state.object.index, state.sub, abort_code);
                    }
                    if let Err(abort_code) = objdata.end_partial(state.sub) {
                        return SdoResult::abort(state.object.index, state.sub, abort_code);
                    }
                }

                SdoResult::response_with_update(
                    SdoResponse::ConfirmBlockDownloadEnd,
                    state.object.index,
                    state.sub,
                    SdoState::Idle,
                )
            }
            SdoRequest::Abort {
                index: _,
                sub: _,
                abort_code: _,
            } => SdoResult::no_response(SdoState::Idle),
            _ => SdoResult::abort(
                state.object.index,
                state.sub,
                AbortCode::InvalidCommandSpecifier,
            ),
        }
    }
}

/// Implements an SDO server
///
/// A single SDO server can be controlled by a single SDO client (at one time). This struct wraps up
/// the state and implements handling of SDO requests. A node implementing multiple SDO servers can
/// instantiate multiple instances of `SdoServer` to track each.
pub(crate) struct SdoServer {
    state: SdoState,
}

impl SdoServer {
    /// Create a new SDO server
    pub fn new() -> Self {
        Self {
            state: SdoState::Idle,
        }
    }

    /// Handle incoming SDO requests
    ///
    /// This will process the request, update server state and the object dictionary accordingly,
    /// and return a response to be transmitted back to the client, as well the index of the updated
    /// object when a download is completed.
    pub fn process(
        &mut self,
        rx: &SdoReceiver,
        elapsed_us: u32,
        od: &'static [ODEntry<'static>],
    ) -> (Option<SdoResponse>, Option<ObjectId>) {
        let result = self.state.update(rx, elapsed_us, od);
        self.state = result.new_state;
        (result.response, result.updated_object)
    }
}

#[cfg(test)]
mod tests {
    use crate::object_dict::{
        find_object, ByteField, ConstField, NullTermByteField, ProvidesSubObjects, SubObjectAccess,
    };
    use zencan_common::{
        objects::{AccessType, DataType, ObjectCode},
        sdo::BlockSegment,
    };

    use crate::SDO_BUFFER_SIZE;

    use super::*;

    const SUB2_SIZE: usize = 78;
    struct Object1000 {
        sub1: NullTermByteField<1200>,
        sub2: ByteField<SUB2_SIZE>,
    }

    impl ProvidesSubObjects for Object1000 {
        fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
            match sub {
                0 => Some((
                    SubInfo::MAX_SUB_NUMBER,
                    const { &ConstField::new(1u8.to_le_bytes()) },
                )),
                1 => Some((
                    SubInfo {
                        size: self.sub1.len(),
                        data_type: DataType::VisibleString,
                        access_type: AccessType::Rw,
                        ..Default::default()
                    },
                    &self.sub1,
                )),
                2 => Some((
                    SubInfo {
                        size: self.sub2.len(),
                        data_type: DataType::OctetString,
                        access_type: AccessType::Rw,
                        ..Default::default()
                    },
                    &self.sub2,
                )),
                _ => None,
            }
        }

        fn object_code(&self) -> ObjectCode {
            ObjectCode::Record
        }
    }

    fn test_od() -> &'static [ODEntry<'static>] {
        let object1000 = Box::leak(Box::new(Object1000 {
            sub1: NullTermByteField::new([0; 1200]),
            sub2: ByteField::new([0; SUB2_SIZE]),
        }));
        let list = [ODEntry {
            index: 0x1000,
            data: object1000,
        }];

        Box::leak(Box::new(list))
    }

    fn do_happy_block_download(
        server: &mut SdoServer,
        rx: &SdoReceiver,
        od: &'static [ODEntry<'static>],
        size: usize,
    ) {
        const INDEX: u16 = 0x1000;
        const SUB: u8 = 1;
        let mut round_trip = |msg_data: [u8; 8], elapsed| {
            rx.handle_req(&msg_data);
            server.process(&rx, elapsed, od)
        };

        let msg = SdoRequest::initiate_block_download(INDEX, SUB, true, size as u32).to_bytes();
        let (resp, index) = round_trip(msg, 0);
        assert_eq!(
            resp,
            Some(SdoResponse::ConfirmBlockDownload {
                sc: true,
                index: INDEX,
                sub: SUB,
                blksize: 127
            })
        );
        assert_eq!(None, index);

        let mut pos = 0;
        let mut seqnum = 0;

        let data = Vec::from_iter((0..size).map(|x| (x % 256) as u8));
        let crc = crc16::State::<crc16::XMODEM>::calculate(&data);
        while pos < size {
            let len = (size - pos).min(7);
            let mut chunk = [0; 7];
            chunk[0..len].copy_from_slice(&data[pos..pos + len as usize]);

            pos += len;
            seqnum += 1;

            let c = pos == size;

            let msg = BlockSegment {
                c,
                seqnum,
                data: chunk.try_into().unwrap(),
            }
            .to_bytes();

            let (resp, index) = round_trip(msg, 0);

            if c {
                assert_eq!(
                    Some(SdoResponse::ConfirmBlock {
                        ackseq: seqnum,
                        blksize: 127
                    }),
                    resp
                );
            } else if seqnum == 127 {
                // start a new block
                seqnum = 0;
                assert_eq!(
                    Some(SdoResponse::ConfirmBlock {
                        ackseq: 127,
                        blksize: 127
                    }),
                    resp
                );
            } else {
                assert_eq!(None, resp);
            }
            assert_eq!(None, index);
        }

        let n = ((7 - size % 7) % 7) as u8;
        let msg = SdoRequest::end_block_download(n, crc).to_bytes();
        let (resp, index) = round_trip(msg, 0);
        assert_eq!(Some(SdoResponse::ConfirmBlockDownloadEnd), resp);
        assert_eq!(
            Some(ObjectId {
                index: INDEX,
                sub: SUB
            }),
            index
        );

        let mut read_buf = vec![0u8; size];
        od[0].data.read(SUB, 0, &mut read_buf).unwrap();
        assert_eq!(data, read_buf);
    }

    #[test]
    fn test_block_download() {
        let buffer = Box::leak(Box::new([0; SDO_BUFFER_SIZE]));
        let mut server = SdoServer::new();
        let rx = SdoReceiver::new(buffer);
        let od = test_od();

        println!("Running 128 byte download");
        do_happy_block_download(&mut server, &rx, od, 128);
        println!("Running 1200 byte download");
        do_happy_block_download(&mut server, &rx, od, 1200);
    }

    #[test]
    fn test_block_download_missing_block() {
        let buffer = Box::leak(Box::new([0; SDO_BUFFER_SIZE]));
        let mut server = SdoServer::new();
        let rx = SdoReceiver::new(buffer);
        let od = test_od();

        const INDEX: u16 = 0x1000;
        const SUB: u8 = 1;
        const DATA_SIZE: usize = 7 * 3;
        let mut round_trip = |msg_data: [u8; 8], elapsed| {
            rx.handle_req(&msg_data);
            server.process(&rx, elapsed, od)
        };

        let mut data = [0; DATA_SIZE];
        for i in 0..DATA_SIZE {
            data[i] = i as u8;
        }

        // Initiate the transfer
        let msg =
            SdoRequest::initiate_block_download(INDEX, SUB, true, DATA_SIZE as u32).to_bytes();
        let (resp, index) = round_trip(msg, 0);
        assert_eq!(
            resp,
            Some(SdoResponse::ConfirmBlockDownload {
                sc: true,
                index: INDEX,
                sub: SUB,
                blksize: 127
            })
        );
        assert_eq!(None, index);

        // Send the first block, and the third block, skipping the second block
        let (resp, index) = round_trip(
            BlockSegment {
                c: false,
                seqnum: 1,
                data: data[0..7].try_into().unwrap(),
            }
            .to_bytes(),
            0,
        );
        assert_eq!(None, resp);
        assert_eq!(None, index);

        let (resp, index) = round_trip(
            BlockSegment {
                c: true,
                seqnum: 3,
                data: data[14..21].try_into().unwrap(),
            }
            .to_bytes(),
            0,
        );
        assert_eq!(
            Some(SdoResponse::ConfirmBlock {
                ackseq: 1,
                blksize: 127
            }),
            resp
        );
        assert_eq!(None, index);

        // Retransmit the last two blocks
        let (resp, index) = round_trip(
            BlockSegment {
                c: false,
                seqnum: 2,
                data: data[7..14].try_into().unwrap(),
            }
            .to_bytes(),
            0,
        );
        assert_eq!(None, resp);
        assert_eq!(None, index);

        let (resp, index) = round_trip(
            BlockSegment {
                c: true,
                seqnum: 3,
                data: data[14..21].try_into().unwrap(),
            }
            .to_bytes(),
            0,
        );
        assert_eq!(
            Some(SdoResponse::ConfirmBlock {
                ackseq: 3,
                blksize: 127
            }),
            resp
        );
        assert_eq!(None, index);

        // End the transfer and check the value in the object
        let n = ((7 - DATA_SIZE % 7) % 7) as u8;
        let crc = crc16::State::<crc16::XMODEM>::calculate(&data);
        let msg = SdoRequest::end_block_download(n, crc).to_bytes();
        let (resp, index) = round_trip(msg, 0);
        assert_eq!(Some(SdoResponse::ConfirmBlockDownloadEnd), resp);
        assert_eq!(
            Some(ObjectId {
                index: INDEX,
                sub: SUB
            }),
            index
        );

        let mut read_buf = vec![0u8; DATA_SIZE];
        od[0].data.read(SUB, 0, &mut read_buf).unwrap();
        assert_eq!(data.as_slice(), read_buf);
    }

    #[test]
    fn test_block_download_timeout() {
        let buffer = Box::leak(Box::new([0; SDO_BUFFER_SIZE]));
        let mut server = SdoServer::new();
        let rx = SdoReceiver::new(buffer);
        let od = test_od();

        const INDEX: u16 = 0x1000;
        const SUB: u8 = 1;
        const DATA_SIZE: usize = 7 * 3;
        let mut round_trip = |msg_data: Option<[u8; 8]>, elapsed| {
            if let Some(msg_data) = msg_data {
                rx.handle_req(&msg_data);
            }
            server.process(&rx, elapsed, od)
        };

        let mut data = [0; DATA_SIZE];
        for i in 0..DATA_SIZE {
            data[i] = i as u8;
        }

        // Start transfer
        let (resp, index) = round_trip(
            Some(
                SdoRequest::initiate_block_download(INDEX, SUB, true, DATA_SIZE as u32).to_bytes(),
            ),
            0,
        );
        assert_eq!(
            resp,
            Some(SdoResponse::ConfirmBlockDownload {
                sc: true,
                index: INDEX,
                sub: SUB,
                blksize: 127
            })
        );
        assert_eq!(None, index);

        // After a small amount of time, we should have no response
        let (resp, index) = round_trip(None, 1000);
        assert_eq!(None, resp);
        assert_eq!(None, index);

        // After a long time, it should time out and send an abort
        let (resp, index) = round_trip(None, 1000000);
        assert_eq!(
            Some(SdoResponse::Abort {
                index: INDEX,
                sub: SUB,
                abort_code: AbortCode::SdoTimeout as u32
            }),
            resp
        );
        assert_eq!(None, index);
    }

    /// Test uploading a value with a length of 7
    #[test]
    fn test_segmented_download() {
        const SDO_BUFFER_SIZE: usize = 32;
        let buffer = Box::leak(Box::new([0; SDO_BUFFER_SIZE]));
        let mut server = SdoServer::new();
        let rx = SdoReceiver::new(buffer);
        let od = test_od();

        const INDEX: u16 = 0x1000;
        const SUB: u8 = 2;

        let mut round_trip = |msg_data: Option<[u8; 8]>, elapsed| {
            if let Some(msg_data) = msg_data {
                rx.handle_req(&msg_data);
            }
            server.process(&rx, elapsed, od)
        };

        let mut do_segmented_download = |size: usize| {
            let write_data = Vec::from_iter((0..size).map(|x| x as u8));
            let mut toggle = false;
            let mut sent_bytes = 0;
            let (resp, index) = round_trip(
                Some(SdoRequest::initiate_download(INDEX, SUB, Some(7)).to_bytes()),
                0,
            );

            assert_eq!(None, index);
            assert_eq!(
                Some(SdoResponse::ConfirmDownload {
                    index: INDEX,
                    sub: SUB
                }),
                resp
            );

            while sent_bytes < write_data.len() {
                let bytes_left = write_data.len() - sent_bytes;
                let bytes_to_send = bytes_left.min(7);
                let complete = bytes_left <= bytes_to_send;
                let (resp, index) = round_trip(
                    Some(
                        SdoRequest::download_segment(
                            toggle,
                            complete,
                            &write_data[sent_bytes..sent_bytes + bytes_to_send],
                        )
                        .to_bytes(),
                    ),
                    0,
                );
                assert_eq!(
                    Some(SdoResponse::ConfirmDownloadSegment { t: toggle }),
                    resp
                );
                if complete {
                    assert_eq!(
                        Some(ObjectId {
                            index: INDEX,
                            sub: SUB
                        }),
                        index
                    );
                } else {
                    assert_eq!(None, index);
                }
                toggle = !toggle;
                sent_bytes += bytes_to_send;
            }

            // Grab the object and read back the data we just wrote
            let obj = find_object(od, INDEX).unwrap();
            let mut read_buf = vec![0; write_data.len()];
            let read_size = obj.read(SUB, 0, &mut read_buf).unwrap();
            assert_eq!(write_data.len(), read_size);
            assert_eq!(write_data, read_buf);
        };

        // Test downloading a single segment object smaller than segment
        do_segmented_download(6);
        // Test downloading a 7 byte, single segment object
        do_segmented_download(7);
        // Test downloading a single segment object just bigger than one segment
        do_segmented_download(8);
        // Test doing a length equal to the SDO buffer size
        do_segmented_download(SDO_BUFFER_SIZE);
        // Test doing a length just larger than the buffer
        do_segmented_download(SDO_BUFFER_SIZE + 1);
        // Tests full object write
        do_segmented_download(SUB2_SIZE);
    }
}
