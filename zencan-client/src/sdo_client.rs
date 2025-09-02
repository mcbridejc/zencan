use std::time::Duration;

use snafu::Snafu;
use zencan_common::{
    constants::{object_ids, values::SAVE_CMD},
    lss::LssIdentity,
    messages::CanId,
    sdo::{AbortCode, BlockSegment, SdoRequest, SdoResponse},
    traits::{AsyncCanReceiver, AsyncCanSender},
};

use crate::node_configuration::PdoConfig;

const RESPONSE_TIMEOUT: Duration = Duration::from_millis(100);

/// A wrapper around the AbortCode enum to allow for unknown values
///
/// Although the library should "know" all the abort codes, it is possible to receive other values
/// and this allows those to be captured and exposed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RawAbortCode {
    /// A recognized abort code
    Valid(AbortCode),
    /// An unrecognized abort code
    Unknown(u32),
}

impl std::fmt::Display for RawAbortCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RawAbortCode::Valid(abort_code) => write!(f, "{abort_code:?}"),
            RawAbortCode::Unknown(code) => write!(f, "{code:X}"),
        }
    }
}

impl From<u32> for RawAbortCode {
    fn from(value: u32) -> Self {
        match AbortCode::try_from(value) {
            Ok(code) => Self::Valid(code),
            Err(_) => Self::Unknown(value),
        }
    }
}

/// Error returned by [`SdoClient`] methods
#[derive(Clone, Debug, PartialEq, Snafu)]
pub enum SdoClientError {
    /// Timeout while awaiting an expected response
    NoResponse,
    /// Received a response that could not be interpreted
    MalformedResponse,
    /// Received a valid SdoResponse, but with an unexpected command specifier
    #[snafu(display("Unexpected SDO response. Expected {expecting}, got {response:?}"))]
    UnexpectedResponse {
        /// The type of response which was expected
        expecting: String,
        /// The response which was received
        response: SdoResponse,
    },
    /// Received a ServerAbort response from the node
    #[snafu(display("Received abort accessing object 0x{index:X}sub{sub}: {abort_code}"))]
    ServerAbort {
        /// Index of the SDO access which was aborted
        index: u16,
        /// Sub index of the SDO access which was aborted
        sub: u8,
        /// Reason for the abort
        abort_code: RawAbortCode,
    },
    /// Received a response with the wrong toggle bit
    ToggleNotAlternated,
    /// Received a response with a different index/sub value than was requested
    #[snafu(display("Received object 0x{:x}sub{} after requesting 0x{:x}sub{}",
        received.0, received.1, expected.0, expected.1))]
    MismatchedObjectIndex {
        /// The object ID which was expected to be echoed back
        expected: (u16, u8),
        /// The received object ID
        received: (u16, u8),
    },
    /// An SDO upload response had a size that did not match the expected size
    UnexpectedSize,
    /// Failed to write a message to the socket
    #[snafu(display("Error sending CAN message"))]
    SocketSendFailed,
    /// An SDO server shrunk the block size while requesting retransmission
    ///
    /// Hopefully no node will ever do this, but it's a possible corner case, since servers are
    /// allowed to change the block size between each block, and can request resend of part of a
    /// block by not acknowledging all segments.
    BlockSizeChangedTooSmall,
}

type Result<T> = std::result::Result<T, SdoClientError>;

/// Convenience macro for expecting a particular variant of a response and erroring on abort of
/// unexpected variant
macro_rules! match_response  {
    ($resp: ident, $expecting: literal, $($match:pat => $code : expr),*) => {
                match $resp {
                    $($match => $code),*
                    SdoResponse::Abort {
                        index,
                        sub,
                        abort_code,
                    } => {
                        return ServerAbortSnafu {
                            index,
                            sub,
                            abort_code,
                        }
                        .fail()
                    }
                    _ => {
                        return UnexpectedResponseSnafu {
                            expecting: $expecting,
                            response: $resp,
                        }
                        .fail()
                    }
                }
    };
}

#[derive(Debug)]
/// A client for accessing a node's SDO server
///
/// A single server can talk to a single client at a time.
pub struct SdoClient<S, R> {
    req_cob_id: CanId,
    resp_cob_id: CanId,
    sender: S,
    receiver: R,
}

impl<S: AsyncCanSender, R: AsyncCanReceiver> SdoClient<S, R> {
    /// Create a new SdoClient using a node ID
    ///
    /// Nodes have a default SDO server, which uses a COB ID based on the node ID. This is a
    /// shortcut to create a client that that default SDO server.
    ///
    /// It is possible for nodes to have other SDO servers on other COB IDs, and clients for these
    /// can be created using [`Self::new()`]
    pub fn new_std(server_node_id: u8, sender: S, receiver: R) -> Self {
        let req_cob_id = CanId::Std(0x600 + server_node_id as u16);
        let resp_cob_id = CanId::Std(0x580 + server_node_id as u16);
        Self::new(req_cob_id, resp_cob_id, sender, receiver)
    }

    /// Create a new SdoClient from request and response COB IDs
    pub fn new(req_cob_id: CanId, resp_cob_id: CanId, sender: S, receiver: R) -> Self {
        Self {
            req_cob_id,
            resp_cob_id,
            sender,
            receiver,
        }
    }

    /// Write data to a sub-object on the SDO server
    pub async fn download(&mut self, index: u16, sub: u8, data: &[u8]) -> Result<()> {
        if data.len() <= 4 {
            // Do an expedited transfer
            let msg =
                SdoRequest::expedited_download(index, sub, data).to_can_message(self.req_cob_id);
            self.sender.send(msg).await.unwrap(); // TODO: Expect errors

            let resp = self.wait_for_response(RESPONSE_TIMEOUT).await?;
            match_response!(
                resp,
                "ConfirmDownload",
                SdoResponse::ConfirmDownload { index: _, sub: _ } => {
                    Ok(()) // Success!
                }
            )
        } else {
            let msg = SdoRequest::initiate_download(index, sub, Some(data.len() as u32))
                .to_can_message(self.req_cob_id);
            self.sender.send(msg).await.unwrap();

            let resp = self.wait_for_response(RESPONSE_TIMEOUT).await?;
            match_response!(
                resp,
                "ConfirmDownload",
                SdoResponse::ConfirmDownload { index: _, sub: _ } => { }
            );

            let mut toggle = false;
            // Send segments
            let total_segments = data.len().div_ceil(7);
            for n in 0..total_segments {
                let last_segment = n == total_segments - 1;
                let segment_size = (data.len() - n * 7).min(7);
                let seg_msg = SdoRequest::download_segment(
                    toggle,
                    last_segment,
                    &data[n * 7..n * 7 + segment_size],
                )
                .to_can_message(self.req_cob_id);
                self.sender
                    .send(seg_msg)
                    .await
                    .expect("failed sending DL segment");
                let resp = self.wait_for_response(RESPONSE_TIMEOUT).await?;
                match_response!(
                    resp,
                    "ConfirmDownloadSegment",
                    SdoResponse::ConfirmDownloadSegment { t } => {
                        // Fail if toggle value doesn't match
                        if t != toggle {
                            let abort_msg =
                                SdoRequest::abort(index, sub, AbortCode::ToggleNotAlternated)
                                    .to_can_message(self.req_cob_id);
                            self.sender
                                .send(abort_msg)
                                .await
                                .expect("Error sending abort");
                            return ToggleNotAlternatedSnafu.fail();
                        }
                        // Otherwise, carry on
                    }
                );
                toggle = !toggle;
            }
            Ok(())
        }
    }

    /// Read a sub-object on the SDO server
    pub async fn upload(&mut self, index: u16, sub: u8) -> Result<Vec<u8>> {
        let mut read_buf = Vec::new();

        let msg = SdoRequest::initiate_upload(index, sub).to_can_message(self.req_cob_id);
        self.sender.send(msg).await.unwrap();

        let resp = self.wait_for_response(RESPONSE_TIMEOUT).await?;

        let expedited = match_response!(
            resp,
            "ConfirmUpload",
            SdoResponse::ConfirmUpload {
                n,
                e,
                s,
                index: _,
                sub: _,
                data,
            } => {
                if e {
                    let mut len = 0;
                    if s {
                        len = 4 - n as usize;
                    }
                    read_buf.extend_from_slice(&data[0..len]);
                }
                e
            }
        );

        if !expedited {
            // Read segments
            let mut toggle = false;
            loop {
                let msg =
                    SdoRequest::upload_segment_request(toggle).to_can_message(self.req_cob_id);

                self.sender.send(msg).await.unwrap();

                let resp = self.wait_for_response(RESPONSE_TIMEOUT).await?;
                match_response!(
                    resp,
                    "UploadSegment",
                    SdoResponse::UploadSegment { t, n, c, data } => {
                        if t != toggle {
                            self.sender
                                .send(
                                    SdoRequest::abort(index, sub, AbortCode::ToggleNotAlternated)
                                        .to_can_message(self.req_cob_id),
                                )
                                .await
                                .expect("Error sending abort");
                            return ToggleNotAlternatedSnafu.fail();
                        }
                        read_buf.extend_from_slice(&data[0..7 - n as usize]);
                        if c {
                            // Transfer complete
                            break;
                        }
                    }
                );
                toggle = !toggle;
            }
        }
        Ok(read_buf)
    }

    /// Perform a block download to transfer data to an object
    ///
    /// Block downloads are more efficient for large amounts of data, but may not be supported by
    /// all devices.
    pub async fn block_download(&mut self, index: u16, sub: u8, data: &[u8]) -> Result<()> {
        self.sender
            .send(
                SdoRequest::InitiateBlockDownload {
                    cc: true, // CRC supported
                    s: true,  // size specified
                    index,
                    sub,
                    size: data.len() as u32,
                }
                .to_can_message(self.req_cob_id),
            )
            .await
            .map_err(|_| SocketSendFailedSnafu {}.build())?;

        let resp = self.wait_for_response(RESPONSE_TIMEOUT).await?;

        let (crc_enabled, mut blksize) = match_response!(
            resp,
            "ConfirmBlockDownload",
            SdoResponse::ConfirmBlockDownload {
                sc,
                index: resp_index,
                sub: resp_sub,
                blksize,
            } => {
                if index != resp_index || sub != resp_sub {
                    return MismatchedObjectIndexSnafu {
                        expected: (index, sub),
                        received: (resp_index, resp_sub),
                    }
                    .fail();
                }
                (sc, blksize)
            }
        );

        let mut seqnum = 1;
        let mut last_block_start = 0;
        let mut segment_num = 0;
        let total_segments = data.len().div_ceil(7);

        while segment_num < total_segments {
            let segment_start = segment_num * 7;
            let segment_len = (data.len() - segment_start).min(7);
            // Is this the last segment?
            let c = segment_start + segment_len == data.len();
            let mut segment_data = [0; 7];
            segment_data[0..segment_len]
                .copy_from_slice(&data[segment_start..segment_start + segment_len]);

            // Send the segment
            let segment = BlockSegment {
                c,
                seqnum,
                data: segment_data,
            };
            self.sender
                .send(segment.to_can_message(self.req_cob_id))
                .await
                .map_err(|_| SocketSendFailedSnafu.build())?;

            // Expect a confirmation message after blksize segments are sent, or after sending the
            // complete flag
            if c || seqnum == blksize {
                let resp = self.wait_for_response(RESPONSE_TIMEOUT).await?;
                match_response!(
                    resp,
                    "ConfirmBlock",
                    SdoResponse::ConfirmBlock {
                        ackseq,
                        blksize: new_blksize,
                    } => {
                        if ackseq == blksize {
                            // All segments are acknowledged. Block accepted
                            seqnum = 1;
                            segment_num += 1;
                            last_block_start = segment_num;
                        } else {
                            // Missing segments. Resend all segments after ackseq
                            seqnum = ackseq;
                            segment_num = last_block_start + ackseq as usize;
                            // The spec says the block size given by the server can change between
                            // blocks. What should a client do if it is going to resend a block, and
                            // the server sets the block size smaller than the already delivered
                            // segments? This shouldn't happen I think, but, it's possible.
                            // zencan-node based nodes won't do it, but there are other devices out
                            // there.
                            if new_blksize < seqnum {
                                return BlockSizeChangedTooSmallSnafu.fail();
                            }
                        }
                        blksize = new_blksize;
                    }
                );
            } else {
                seqnum += 1;
                segment_num += 1;
            }
        }

        // End the download
        let crc = if crc_enabled {
            crc16::State::<crc16::XMODEM>::calculate(data)
        } else {
            0
        };

        let n = ((7 - data.len() % 7) % 7) as u8;

        self.sender
            .send(SdoRequest::EndBlockDownload { n, crc }.to_can_message(self.req_cob_id))
            .await
            .map_err(|_| SocketSendFailedSnafu.build())?;

        let resp = self.wait_for_response(RESPONSE_TIMEOUT).await?;
        match_response!(
            resp,
            "ConfirmBlockDownloadEnd",
            SdoResponse::ConfirmBlockDownloadEnd => { Ok(()) }
        )
    }

    /// Write to a u32 object on the SDO server
    pub async fn download_u32(&mut self, index: u16, sub: u8, data: u32) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Alias for `download_u32`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn write_u32(&mut self, index: u16, sub: u8, data: u32) -> Result<()> {
        self.download_u32(index, sub, data).await
    }

    /// Write to a u16 object on the SDO server
    pub async fn download_u16(&mut self, index: u16, sub: u8, data: u16) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Alias for `download_u16`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn write_u16(&mut self, index: u16, sub: u8, data: u16) -> Result<()> {
        self.download_u16(index, sub, data).await
    }

    /// Write to a u16 object on the SDO server
    pub async fn download_u8(&mut self, index: u16, sub: u8, data: u8) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Alias for `download_u8`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn write_u8(&mut self, index: u16, sub: u8, data: u8) -> Result<()> {
        self.download_u8(index, sub, data).await
    }

    /// Write to an i32 object on the SDO server
    pub async fn download_i32(&mut self, index: u16, sub: u8, data: i32) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Alias for `download_i32`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn write_i32(&mut self, index: u16, sub: u8, data: i32) -> Result<()> {
        self.download_i32(index, sub, data).await
    }

    /// Write to an i16 object on the SDO server
    pub async fn download_i16(&mut self, index: u16, sub: u8, data: i16) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Alias for `download_i16`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn write_i16(&mut self, index: u16, sub: u8, data: i16) -> Result<()> {
        self.download_i16(index, sub, data).await
    }

    /// Write to an i8 object on the SDO server
    pub async fn download_i8(&mut self, index: u16, sub: u8, data: i8) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Alias for `download_i8`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn write_i8(&mut self, index: u16, sub: u8, data: i8) -> Result<()> {
        self.download_i8(index, sub, data).await
    }

    /// Read a string from the SDO server
    pub async fn upload_utf8(&mut self, index: u16, sub: u8) -> Result<String> {
        let data = self.upload(index, sub).await?;
        Ok(String::from_utf8_lossy(&data).into())
    }
    /// Alias for `upload_utf8`
    pub async fn read_utf8(&mut self, index: u16, sub: u8) -> Result<String> {
        self.upload_utf8(index, sub).await
    }

    /// Read a sub-object from the SDO server, assuming it is an u8
    pub async fn upload_u8(&mut self, index: u16, sub: u8) -> Result<u8> {
        let data = self.upload(index, sub).await?;
        if data.len() != 1 {
            return UnexpectedSizeSnafu.fail();
        }
        Ok(data[0])
    }
    /// Alias for `upload_u8`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn read_u8(&mut self, index: u16, sub: u8) -> Result<u8> {
        self.upload_u8(index, sub).await
    }

    /// Read a sub-object from the SDO server, assuming it is an u16
    pub async fn upload_u16(&mut self, index: u16, sub: u8) -> Result<u16> {
        let data = self.upload(index, sub).await?;
        if data.len() != 2 {
            return UnexpectedSizeSnafu.fail();
        }
        Ok(u16::from_le_bytes(data.try_into().unwrap()))
    }

    /// Alias for `upload_u16`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn read_u16(&mut self, index: u16, sub: u8) -> Result<u16> {
        self.upload_u16(index, sub).await
    }

    /// Read a sub-object from the SDO server, assuming it is an u32
    pub async fn upload_u32(&mut self, index: u16, sub: u8) -> Result<u32> {
        let data = self.upload(index, sub).await?;
        if data.len() != 4 {
            return UnexpectedSizeSnafu.fail();
        }
        Ok(u32::from_le_bytes(data.try_into().unwrap()))
    }

    /// Alias for `upload_u32`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn read_u32(&mut self, index: u16, sub: u8) -> Result<u32> {
        self.upload_u32(index, sub).await
    }

    /// Read a sub-object from the SDO server, assuming it is an i8
    pub async fn upload_i8(&mut self, index: u16, sub: u8) -> Result<i8> {
        let data = self.upload(index, sub).await?;
        if data.len() != 1 {
            return UnexpectedSizeSnafu.fail();
        }
        Ok(i8::from_le_bytes(data.try_into().unwrap()))
    }

    /// Alias for `upload_i8`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn read_i8(&mut self, index: u16, sub: u8) -> Result<i8> {
        self.upload_i8(index, sub).await
    }

    /// Read a sub-object from the SDO server, assuming it is an i16
    pub async fn upload_i16(&mut self, index: u16, sub: u8) -> Result<i16> {
        let data = self.upload(index, sub).await?;
        if data.len() != 2 {
            return UnexpectedSizeSnafu.fail();
        }
        Ok(i16::from_le_bytes(data.try_into().unwrap()))
    }

    /// Alias for `upload_i16`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn read_i16(&mut self, index: u16, sub: u8) -> Result<i16> {
        self.upload_i16(index, sub).await
    }

    /// Read a sub-object from the SDO server, assuming it is an i32
    pub async fn upload_i32(&mut self, index: u16, sub: u8) -> Result<i32> {
        let data = self.upload(index, sub).await?;
        if data.len() != 4 {
            return UnexpectedSizeSnafu.fail();
        }
        Ok(i32::from_le_bytes(data.try_into().unwrap()))
    }

    /// Alias for `upload_i32`
    ///
    /// This is a convenience function to allow for a more intuitive API
    pub async fn read_i32(&mut self, index: u16, sub: u8) -> Result<i32> {
        self.upload_i32(index, sub).await
    }

    /// Read an object as a visible string
    ///
    /// It will be read and assumed to contain valid UTF8 characters
    pub async fn read_visible_string(&mut self, index: u16, sub: u8) -> Result<String> {
        let bytes = self.upload(index, sub).await?;
        Ok(String::from_utf8_lossy(&bytes).into())
    }

    /// Read the identity object
    ///
    /// All nodes should implement this object
    pub async fn read_identity(&mut self) -> Result<LssIdentity> {
        let vendor_id = self.upload_u32(object_ids::IDENTITY, 1).await?;
        let product_code = self.upload_u32(object_ids::IDENTITY, 2).await?;
        let revision_number = self.upload_u32(object_ids::IDENTITY, 3).await?;
        let serial = self.upload_u32(object_ids::IDENTITY, 4).await?;
        Ok(LssIdentity::new(
            vendor_id,
            product_code,
            revision_number,
            serial,
        ))
    }

    /// Write object 0x1010sub1 to command all objects be saved
    pub async fn save_objects(&mut self) -> Result<()> {
        self.download_u32(object_ids::SAVE_OBJECTS, 1, SAVE_CMD)
            .await
    }

    /// Read the device name object
    ///
    /// All nodes should implement this object
    pub async fn read_device_name(&mut self) -> Result<String> {
        self.read_visible_string(object_ids::DEVICE_NAME, 0).await
    }

    /// Read the software version object
    ///
    /// All nodes should implement this object
    pub async fn read_software_version(&mut self) -> Result<String> {
        self.read_visible_string(object_ids::SOFTWARE_VERSION, 0)
            .await
    }

    /// Read the hardware version object
    ///
    /// All nodes should implement this object
    pub async fn read_hardware_version(&mut self) -> Result<String> {
        self.read_visible_string(object_ids::HARDWARE_VERSION, 0)
            .await
    }

    /// Configure a transmit PDO on the device
    ///
    /// This is a convenience function to write the PDO comm and mapping objects based on a
    /// [`PdoConfig`].
    pub async fn configure_tpdo(&mut self, pdo_num: usize, cfg: &PdoConfig) -> Result<()> {
        let comm_index = 0x1800 + pdo_num as u16;
        let mapping_index = 0x1a00 + pdo_num as u16;
        self.store_pdo(comm_index, mapping_index, cfg).await
    }

    /// Configure a receive PDO on the device
    ///
    /// This is a convenience function to write the PDO comm and mapping objects based on a
    /// [`PdoConfig`].
    pub async fn configure_rpdo(&mut self, pdo_num: usize, cfg: &PdoConfig) -> Result<()> {
        let comm_index = 0x1400 + pdo_num as u16;
        let mapping_index = 0x1600 + pdo_num as u16;
        self.store_pdo(comm_index, mapping_index, cfg).await
    }

    async fn store_pdo(
        &mut self,
        comm_index: u16,
        mapping_index: u16,
        cfg: &PdoConfig,
    ) -> Result<()> {
        assert!(cfg.mappings.len() < 0x40);
        for (i, m) in cfg.mappings.iter().enumerate() {
            let mapping_value = m.to_object_value();
            self.write_u32(mapping_index, (i + 1) as u8, mapping_value)
                .await?;
        }

        let num_mappings = cfg.mappings.len() as u8;
        self.write_u8(mapping_index, 0, num_mappings).await?;

        let mut cob_value = cfg.cob.raw() & 0xFFFFFFF;
        if !cfg.enabled {
            cob_value |= 1 << 31;
        }
        if cfg.cob.is_extended() {
            cob_value |= 1 << 29;
        }
        self.write_u8(comm_index, 2, cfg.transmission_type).await?;
        self.write_u32(comm_index, 1, cob_value).await?;

        Ok(())
    }

    async fn wait_for_response(&mut self, timeout: Duration) -> Result<SdoResponse> {
        let wait_until = tokio::time::Instant::now() + timeout;
        loop {
            match tokio::time::timeout_at(wait_until, self.receiver.recv()).await {
                // Err indicates the timeout elapsed, so return
                Err(_) => return NoResponseSnafu.fail(),
                // Message was recieved. If it is the resp, return. Otherwise, keep waiting
                Ok(Ok(msg)) => {
                    if msg.id == self.resp_cob_id {
                        return msg.try_into().map_err(|_| MalformedResponseSnafu.build());
                    }
                }
                // Recv returned an error
                Ok(Err(e)) => {
                    log::error!("Error reading from socket: {e:?}");
                    return NoResponseSnafu.fail();
                }
            }
        }
    }
}
