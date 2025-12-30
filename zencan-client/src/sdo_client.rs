use std::time::Duration;

use snafu::Snafu;
use zencan_common::{
    constants::{object_ids, values::SAVE_CMD},
    lss::LssIdentity,
    messages::CanId,
    node_configuration::PdoConfig,
    pdo::PdoMapping,
    sdo::{AbortCode, BlockSegment, SdoRequest, SdoResponse},
    traits::{AsyncCanReceiver, AsyncCanSender, CanSendError as _},
    CanMessage, TimeDifference, TimeOfDay,
};

const DEFAULT_RESPONSE_TIMEOUT: Duration = Duration::from_millis(150);

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
    #[snafu(display("Failed to send CAN message: {message}"))]
    SocketSendFailed {
        /// A string describing the error reason
        message: String,
    },
    /// An SDO server shrunk the block size while requesting retransmission
    ///
    /// Hopefully no node will ever do this, but it's a possible corner case, since servers are
    /// allowed to change the block size between each block, and can request resend of part of a
    /// block by not acknowledging all segments.
    BlockSizeChangedTooSmall,
    /// The CRC on a block upload did not match
    CrcMismatch,
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

use paste::paste;
macro_rules! access_methods {
    ($type: ty) => {

        paste! {
            #[doc = concat!("Read a ", stringify!($type), " sub object from the SDO server\n\n")]
            #[doc = concat!("This is an alias for upload_", stringify!($type), " for a more intuitive API")]
            pub async fn [<read_ $type>](&mut self, index: u16, sub: u8) -> Result<$type> {
                self.[<upload_ $type>](index, sub).await
            }

            #[doc = concat!("Read a ", stringify!($type), " sub object from the SDO server")]
            pub async fn [<upload_ $type>](&mut self, index: u16, sub: u8) -> Result<$type> {
                let data = self.upload(index, sub).await?;
                if data.len() != size_of::<$type>() {
                    return UnexpectedSizeSnafu.fail();
                }
                Ok($type::from_le_bytes(data.try_into().unwrap()))
            }

            #[doc = concat!("Write a ", stringify!($type), " sub object on the SDO server\n\n")]
            #[doc = concat!("This is an alias for download_", stringify!($type), " for a more intuitive API")]
            pub async fn [<write_ $type>](&mut self, index: u16, sub: u8, value: $type) -> Result<()> {
                self.[<download_ $type>](index, sub, value).await
            }

            #[doc = concat!("Read a ", stringify!($type), " sub object from the SDO server")]
            pub async fn [<download_ $type>](&mut self, index: u16, sub: u8, value: $type) -> Result<()> {
                let data = value.to_le_bytes();
                self.download(index, sub, &data).await
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
    timeout: Duration,
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
            timeout: DEFAULT_RESPONSE_TIMEOUT,
            sender,
            receiver,
        }
    }

    /// Set the timeout for waiting on SDO server responses
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Get the current timeout for waiting on SDO server responses
    pub fn get_timeout(&self) -> Duration {
        self.timeout
    }

    async fn send(&mut self, data: [u8; 8]) -> Result<()> {
        let frame = CanMessage::new(self.req_cob_id, &data);
        let mut tries = 3;
        loop {
            match self.sender.send(frame).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tries -= 1;
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    if tries == 0 {
                        return SocketSendFailedSnafu {
                            message: e.message(),
                        }
                        .fail();
                    }
                }
            }
        }
    }

    /// Write data to a sub-object on the SDO server
    pub async fn download(&mut self, index: u16, sub: u8, data: &[u8]) -> Result<()> {
        if data.len() <= 4 {
            // Do an expedited transfer
            self.send(SdoRequest::expedited_download(index, sub, data).to_bytes())
                .await?;

            let resp = self.wait_for_response().await?;
            match_response!(
                resp,
                "ConfirmDownload",
                SdoResponse::ConfirmDownload { index: _, sub: _ } => {
                    Ok(()) // Success!
                }
            )
        } else {
            self.send(
                SdoRequest::initiate_download(index, sub, Some(data.len() as u32)).to_bytes(),
            )
            .await?;

            let resp = self.wait_for_response().await?;
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
                );
                self.send(seg_msg.to_bytes()).await?;
                let resp = self.wait_for_response().await?;
                match_response!(
                    resp,
                    "ConfirmDownloadSegment",
                    SdoResponse::ConfirmDownloadSegment { t } => {
                        // Fail if toggle value doesn't match
                        if t != toggle {
                            let abort_msg =
                                SdoRequest::abort(index, sub, AbortCode::ToggleNotAlternated);

                            self.send(abort_msg.to_bytes())
                                .await?;
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

        self.send(SdoRequest::initiate_upload(index, sub).to_bytes())
            .await?;

        let resp = self.wait_for_response().await?;

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
                self.send(SdoRequest::upload_segment_request(toggle).to_bytes())
                    .await?;

                let resp = self.wait_for_response().await?;
                match_response!(
                    resp,
                    "UploadSegment",
                    SdoResponse::UploadSegment { t, n, c, data } => {
                        if t != toggle {
                            self.send(
                                    SdoRequest::abort(index, sub, AbortCode::ToggleNotAlternated)
                                        .to_bytes(),
                                )
                                .await?;
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
        self.send(
            SdoRequest::InitiateBlockDownload {
                cc: true, // CRC supported
                s: true,  // size specified
                index,
                sub,
                size: data.len() as u32,
            }
            .to_bytes(),
        )
        .await?;

        let resp = self.wait_for_response().await?;

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
            self.send(segment.to_bytes()).await?;

            // Expect a confirmation message after blksize segments are sent, or after sending the
            // complete flag
            if c || seqnum == blksize {
                let resp = self.wait_for_response().await?;
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

        self.send(SdoRequest::EndBlockDownload { n, crc }.to_bytes())
            .await?;

        let resp = self.wait_for_response().await?;
        match_response!(
            resp,
            "ConfirmBlockDownloadEnd",
            SdoResponse::ConfirmBlockDownloadEnd => { Ok(()) }
        )
    }

    /// Perform a block upload of data from the node
    pub async fn block_upload(&mut self, index: u16, sub: u8) -> Result<Vec<u8>> {
        const CRC_SUPPORTED: bool = true;
        const BLKSIZE: u8 = 127;
        const PST: u8 = 0;
        self.send(
            SdoRequest::initiate_block_upload(index, sub, CRC_SUPPORTED, BLKSIZE, PST).to_bytes(),
        )
        .await?;

        let resp = self.wait_for_response().await?;

        let server_supports_crc = match_response!(
            resp,
            "ConfirmBlockUpload",
            SdoResponse::ConfirmBlockUpload { sc, s: _, index: _, sub: _, size: _ } => {sc}
        );

        self.send(SdoRequest::StartBlockUpload.to_bytes()).await?;

        let mut rx_data = Vec::new();
        let last_segment;
        loop {
            let segment = self.wait_for_block_segment().await?;
            rx_data.extend_from_slice(&segment.data);
            if !segment.c && segment.seqnum == BLKSIZE {
                // Finished sub block, but not yet done. Confirm this sub block and expect more
                self.send(
                    SdoRequest::ConfirmBlock {
                        ackseq: BLKSIZE,
                        blksize: BLKSIZE,
                    }
                    .to_bytes(),
                )
                .await?;
            }
            if segment.c {
                last_segment = segment.seqnum;
                break;
            }
        }

        // NOTE: Ignoring the possibility of dropped messages here. Should check seqno to make sure
        // all blocks are received.
        self.send(
            SdoRequest::ConfirmBlock {
                ackseq: last_segment,
                blksize: BLKSIZE,
            }
            .to_bytes(),
        )
        .await?;

        let resp = self.wait_for_response().await?;
        let (n, crc) = match_response!(
            resp,
            "BlockUploadEnd",
            SdoResponse::BlockUploadEnd { n, crc } => {(n, crc)}
        );

        // Drop the n invalid data bytes
        rx_data.resize(rx_data.len() - n as usize, 0);

        if server_supports_crc {
            let computed_crc = crc16::State::<crc16::XMODEM>::calculate(&rx_data);
            if crc != computed_crc {
                self.send(SdoRequest::abort(index, sub, AbortCode::CrcError).to_bytes())
                    .await?;
                return Err(SdoClientError::CrcMismatch);
            }
        }

        self.send(SdoRequest::EndBlockUpload.to_bytes()).await?;

        Ok(rx_data)
    }

    access_methods!(f64);
    access_methods!(f32);
    access_methods!(u64);
    access_methods!(u32);
    access_methods!(u16);
    access_methods!(u8);
    access_methods!(i64);
    access_methods!(i32);
    access_methods!(i16);
    access_methods!(i8);

    /// Write to a TimeOfDay object on the SDO server
    pub async fn download_time_of_day(
        &mut self,
        index: u16,
        sub: u8,
        data: TimeOfDay,
    ) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Write to a TimeOfDay object on the SDO server
    ///
    /// Alias for `download_time_of_day`. This is a convenience function to allow for a more intuitive API.
    pub async fn write_time_of_day(&mut self, index: u16, sub: u8, data: TimeOfDay) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Write to a TimeDifference object on the SDO server
    pub async fn download_time_difference(
        &mut self,
        index: u16,
        sub: u8,
        data: TimeDifference,
    ) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
    }

    /// Write to a TimeDifference object on the SDO server
    ///
    /// Alias for `download_time_difference`. This is a convenience function to allow for a more intuitive API.
    pub async fn write_time_difference(
        &mut self,
        index: u16,
        sub: u8,
        data: TimeDifference,
    ) -> Result<()> {
        let data = data.to_le_bytes();
        self.download(index, sub, &data).await
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

    /// Read a TimeOfDay object from the SDO server
    pub async fn upload_time_of_day(&mut self, index: u16, sub: u8) -> Result<TimeOfDay> {
        let data = self.upload(index, sub).await?;
        if data.len() != TimeOfDay::SIZE {
            UnexpectedSizeSnafu.fail()
        } else {
            Ok(TimeOfDay::from_le_bytes(data.try_into().unwrap()))
        }
    }

    /// Read a TimeOfDay object from the SDO server
    ///
    /// Alias for `upload_time_of_day`. This is a convenience function to allow for a more intuitive
    /// API.
    pub async fn read_time_of_day(&mut self, index: u16, sub: u8) -> Result<TimeOfDay> {
        self.upload_time_of_day(index, sub).await
    }

    /// Read a TimeOfDay object from the SDO server
    pub async fn upload_time_difference(&mut self, index: u16, sub: u8) -> Result<TimeDifference> {
        let data = self.upload(index, sub).await?;
        if data.len() != TimeDifference::SIZE {
            UnexpectedSizeSnafu.fail()
        } else {
            Ok(TimeDifference::from_le_bytes(data.try_into().unwrap()))
        }
    }

    /// Read a TimeOfDay object from the SDO server
    ///
    /// Alias for `upload_time_of_day`. This is a convenience function to allow for a more intuitive
    /// API.
    pub async fn read_time_difference(&mut self, index: u16, sub: u8) -> Result<TimeDifference> {
        self.upload_time_difference(index, sub).await
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

        let mut cob_value = cfg.cob_id.raw() & 0x1FFFFFFF;
        if !cfg.enabled {
            cob_value |= 1 << 31;
        }
        if cfg.cob_id.is_extended() {
            cob_value |= 1 << 29;
        }
        self.write_u8(comm_index, 2, cfg.transmission_type).await?;
        self.write_u32(comm_index, 1, cob_value).await?;

        Ok(())
    }

    /// Read the configuration of an RPDO from the node
    pub async fn read_rpdo_config(&mut self, pdo_num: usize) -> Result<PdoConfig> {
        let comm_index = 0x1400 + pdo_num as u16;
        let mapping_index = 0x1600 + pdo_num as u16;
        self.read_pdo_config(comm_index, mapping_index).await
    }

    /// Read the configuration of a TPDO from the node
    pub async fn read_tpdo_config(&mut self, pdo_num: usize) -> Result<PdoConfig> {
        let comm_index = 0x1800 + pdo_num as u16;
        let mapping_index = 0x1a00 + pdo_num as u16;
        self.read_pdo_config(comm_index, mapping_index).await
    }

    async fn read_pdo_config(&mut self, comm_index: u16, mapping_index: u16) -> Result<PdoConfig> {
        let cob_word = self.read_u32(comm_index, 1).await?;
        let transmission_type = self.read_u8(comm_index, 2).await?;
        let num_mappings = self.read_u8(mapping_index, 0).await?;
        let mut mappings = Vec::with_capacity(num_mappings as usize);
        for i in 0..num_mappings {
            let mapping_raw = self.read_u32(mapping_index, i + 1).await?;
            mappings.push(PdoMapping::from_object_value(mapping_raw));
        }
        let enabled = cob_word & (1 << 31) == 0;
        let rtr_disabled = cob_word & (1 << 30) != 0;
        let extended = cob_word & (1 << 29) != 0;
        let cob_id = cob_word & 0x1FFFFFFF;
        let cob_id = if extended {
            CanId::extended(cob_id)
        } else {
            CanId::std(cob_id as u16)
        };
        Ok(PdoConfig {
            cob_id,
            enabled,
            rtr_disabled,
            mappings,
            transmission_type,
        })
    }

    async fn wait_for_block_segment(&mut self) -> Result<BlockSegment> {
        let wait_until = tokio::time::Instant::now() + self.timeout;
        loop {
            match tokio::time::timeout_at(wait_until, self.receiver.recv()).await {
                // Err indicates the timeout elapsed, so return
                Err(_) => return NoResponseSnafu.fail(),
                // Message was recieved. If it is the resp, return. Otherwise, keep waiting
                Ok(Ok(msg)) => {
                    if msg.id == self.resp_cob_id {
                        return msg
                            .data()
                            .try_into()
                            .map_err(|_| MalformedResponseSnafu.build());
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

    async fn wait_for_response(&mut self) -> Result<SdoResponse> {
        let wait_until = tokio::time::Instant::now() + self.timeout;
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
