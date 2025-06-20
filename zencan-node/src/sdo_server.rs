use zencan_common::{
    objects::{find_object, ODEntry, ObjectRawAccess, SubInfo},
    sdo::{AbortCode, SdoRequest, SdoResponse},
};

#[derive(Copy, Clone, Debug, Default, PartialEq)]
enum State {
    #[default]
    Idle,
    DownloadSegment,
    UploadSegment,
}

/// Implements an SDO server
///
/// A single SDO server can be controlled by a single SDO client (at one time). This struct wraps up
/// the state and implements handling of SDO requests. A node implementing multiple SDO servers can
/// instantiate multiple instances of `SdoServer` to track each.
pub struct SdoServer {
    toggle_state: bool,
    state: State,
    segment_counter: u16,
    index: u16,
    sub: u8,
}

impl Default for SdoServer {
    fn default() -> Self {
        Self::new()
    }
}

impl SdoServer {
    /// Create a new SDO server
    pub fn new() -> Self {
        let toggle_state = false;
        let state = State::Idle;
        let segment_counter = 0;
        let index = 0;
        let sub = 0;
        Self {
            toggle_state,
            state,
            segment_counter,
            index,
            sub,
        }
    }

    fn validate_download_size(&self, dl_size: usize, subobj: &SubInfo) -> Result<(), SdoResponse> {
        if subobj.data_type.is_str() {
            // Strings can write shorter lengths
            if dl_size > subobj.size {
                return Err(SdoResponse::abort(
                    self.index,
                    self.sub,
                    AbortCode::DataTypeMismatchLengthHigh,
                ));
            }
        } else {
            // All other types require exact size
            if dl_size < subobj.size {
                return Err(SdoResponse::abort(
                    self.index,
                    self.sub,
                    AbortCode::DataTypeMismatchLengthLow,
                ));
            } else if dl_size > subobj.size {
                return Err(SdoResponse::abort(
                    self.index,
                    self.sub,
                    AbortCode::DataTypeMismatchLengthHigh,
                ));
            }
        }
        Ok(())
    }

    /// Handle an SDO request received from the client
    ///
    /// This will process the request, update server state and the object dictionary accordingly,
    /// and return a response to be transmitted back to the client, as well the index of the updated
    /// object when a download is completed.
    pub fn handle_request(
        &mut self,
        req: &SdoRequest,
        od: &[ODEntry],
    ) -> (Option<SdoResponse>, Option<u16>) {
        match req {
            SdoRequest::InitiateUpload { index, sub } => {
                let obj = match find_object(od, *index) {
                    Some(x) => x,
                    None => {
                        return (
                            Some(SdoResponse::abort(*index, *sub, AbortCode::NoSuchObject)),
                            None,
                        )
                    }
                };

                let mut buf = [0u8; 4];
                self.toggle_state = false;
                let current_size = match obj.current_size(*sub) {
                    Ok(s) => s,
                    Err(abort_code) => {
                        return (Some(SdoResponse::abort(*index, *sub, abort_code)), None)
                    }
                };

                if current_size <= 4 {
                    self.state = State::Idle;
                    // Do expedited upload
                    if let Err(abort_code) = obj.read(*sub, 0, &mut buf[0..current_size]) {
                        return (Some(SdoResponse::abort(*index, *sub, abort_code)), None);
                    }

                    (
                        Some(SdoResponse::expedited_upload(
                            *index,
                            *sub,
                            &buf[0..current_size],
                        )),
                        None,
                    )
                } else {
                    // Segmented upload
                    self.state = State::UploadSegment;
                    self.index = *index;
                    self.sub = *sub;
                    self.segment_counter = 0;
                    (
                        Some(SdoResponse::upload_acknowledge(
                            *index,
                            *sub,
                            current_size as u32,
                        )),
                        None,
                    )
                }
            }
            SdoRequest::InitiateDownload {
                n,
                e,
                s,
                index,
                sub,
                data,
            } => {
                self.index = *index;
                self.sub = *sub;
                if *e {
                    // Doing an expedited download
                    let obj = match find_object(od, *index) {
                        Some(x) => x,
                        None => {
                            return (
                                Some(SdoResponse::abort(*index, *sub, AbortCode::NoSuchObject)),
                                None,
                            )
                        }
                    };

                    let subinfo = match obj.sub_info(*sub) {
                        Ok(s) => s,
                        Err(abort_code) => {
                            return (Some(SdoResponse::abort(*index, *sub, abort_code)), None)
                        }
                    };
                    // Verify that the requested object is writable
                    if !subinfo.access_type.is_writable() {
                        return (
                            Some(SdoResponse::abort(
                                self.index,
                                self.sub,
                                AbortCode::ReadOnly,
                            )),
                            None,
                        );
                    }

                    // Verify data size requested by client fits object, and abort if not
                    let dl_size = 4 - *n as usize;
                    if let Err(abort_resp) = self.validate_download_size(dl_size, &subinfo) {
                        self.state = State::Idle;
                        return (Some(abort_resp), None);
                    }

                    if let Err(abort_code) = obj.write(*sub, 0, &data[0..dl_size]) {
                        return (Some(SdoResponse::abort(*index, *sub, abort_code)), None);
                    }
                    // When writing a string with length less than buffer, zero terminate
                    // Note: dl_size != subobj.size implies the data type of the object is a string
                    if dl_size < subinfo.size {
                        if let Err(abort_code) = obj.write(*sub, dl_size, &[0]) {
                            return (Some(SdoResponse::abort(*index, *sub, abort_code)), None);
                        }
                    }

                    (
                        Some(SdoResponse::download_acknowledge(*index, *sub)),
                        Some(*index),
                    )
                } else {
                    // starting a segmented download
                    let obj = match find_object(od, *index) {
                        Some(x) => x,
                        None => {
                            return (
                                Some(SdoResponse::abort(*index, *sub, AbortCode::NoSuchObject)),
                                None,
                            )
                        }
                    };
                    let subinfo = match obj.sub_info(*sub) {
                        Ok(s) => s,
                        Err(abort_code) => {
                            return (Some(SdoResponse::abort(*index, *sub, abort_code)), None)
                        }
                    };

                    // If size is provided, verify data size requested by client fits object, and
                    // abort if not
                    if *s {
                        let dl_size = 4 - *n as usize;
                        if let Err(abort_resp) = self.validate_download_size(dl_size, &subinfo) {
                            self.state = State::Idle;
                            return (Some(abort_resp), None);
                        }
                    }

                    self.toggle_state = false;
                    self.segment_counter = 0;
                    self.state = State::DownloadSegment;

                    (Some(SdoResponse::download_acknowledge(*index, *sub)), None)
                }
            }
            SdoRequest::DownloadSegment { t, n, c, data } => {
                if self.state != State::DownloadSegment {
                    self.state = State::Idle;
                    return (
                        Some(SdoResponse::abort(
                            self.index,
                            self.sub,
                            AbortCode::InvalidCommandSpecifier,
                        )),
                        None,
                    );
                }

                if *t != self.toggle_state {
                    self.state = State::Idle;
                    return (
                        Some(SdoResponse::abort(
                            self.index,
                            self.sub,
                            AbortCode::ToggleNotAlternated,
                        )),
                        None,
                    );
                }

                // Unwrap safety: If in DownloadSegment state, then the existence of the sub object
                // is already established.
                let obj = find_object(od, self.index).unwrap();
                // Unwrap safety: see above
                let subinfo = obj.sub_info(self.sub).unwrap();

                let offset = self.segment_counter as usize * 7;
                let segment_size = 7 - *n as usize;
                let write_len = offset + segment_size;
                // Make sure this segment won't overrun the allocated storage
                if write_len > subinfo.size {
                    self.state = State::Idle;
                    return (
                        Some(SdoResponse::abort(
                            self.index,
                            self.sub,
                            AbortCode::DataTypeMismatchLengthHigh,
                        )),
                        None,
                    );
                }
                // Unwrap safety: Both existence and size of the sub object are already checked
                obj.write(self.sub, offset, &data[0..segment_size]).unwrap();
                // If this is the last segment, and it's shorter than the object, zero terminate
                if *c && write_len < subinfo.size {
                    obj.write(self.sub, write_len, &[0]).unwrap();
                }
                self.toggle_state = !self.toggle_state;
                self.segment_counter += 1;
                // Return the updated index if this is the last segment
                let updated_index = if *c { Some(self.index) } else { None };
                (
                    Some(SdoResponse::download_segment_acknowledge(
                        !self.toggle_state,
                    )),
                    updated_index,
                )
            }

            SdoRequest::ReqUploadSegment { t } => {
                if self.state != State::UploadSegment {
                    self.state = State::Idle;
                    return (
                        Some(SdoResponse::abort(
                            self.index,
                            self.sub,
                            AbortCode::InvalidCommandSpecifier,
                        )),
                        None,
                    );
                }
                if *t != self.toggle_state {
                    self.state = State::Idle;
                    return (
                        Some(SdoResponse::abort(
                            self.index,
                            self.sub,
                            AbortCode::ToggleNotAlternated,
                        )),
                        None,
                    );
                }

                // Unwrap safety: If in DownloadSegment state, then the existence of the sub object
                // is already established.
                let obj = find_object(od, self.index).unwrap();
                // Unwrap safety: see above
                let current_size = obj.current_size(self.sub).unwrap();

                let read_offset = self.segment_counter as usize * 7;
                let read_size = (current_size - read_offset).min(7);
                let mut buf = [0; 7];
                obj.read(
                    self.sub,
                    self.segment_counter as usize * 7,
                    &mut buf[0..read_size],
                )
                .unwrap();
                // Compute complete bit (is this the last segment of the upload?)
                let c = (read_size + read_offset) == current_size;
                self.segment_counter += 1;
                self.toggle_state = !self.toggle_state;
                if c {
                    self.state = State::Idle;
                }
                (
                    Some(SdoResponse::upload_segment(
                        !self.toggle_state,
                        c,
                        &buf[0..read_size],
                    )),
                    None,
                )
            }
            SdoRequest::InitiateBlockDownload {
                cc: _,
                s: _,
                cs: _,
                index: _,
                sub: _,
                size: _,
            } => todo!(),
            SdoRequest::InitiateBlockUpload {} => todo!(),
            SdoRequest::Abort {
                index: _,
                sub: _,
                abort_code: _,
            } => {
                self.state = State::Idle;
                // No response is sent to an abort command
                (None, None)
            }
        }
    }
}
