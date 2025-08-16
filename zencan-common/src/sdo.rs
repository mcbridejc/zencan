//! Common SDO implementation
//!
//! Defines messages, constants, etc for SDO protocol
use int_enum::IntEnum;

use crate::messages::{CanId, CanMessage};

/// Specifies the possible server command specifier (SCS) values in SDO response packets
enum ServerCommand {
    SegmentUpload = 0,
    SegmentDownload = 1,
    Upload = 2,
    Download = 3,
    Abort = 4,
    BlockDownload = 5,
    BlockUpload = 6,
}

impl TryFrom<u8> for ServerCommand {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use ServerCommand::*;
        match value {
            0 => Ok(SegmentUpload),
            1 => Ok(SegmentDownload),
            2 => Ok(Upload),
            3 => Ok(Download),
            4 => Ok(Abort),
            5 => Ok(BlockDownload),
            6 => Ok(BlockUpload),
            _ => Err(()),
        }
    }
}

/// SDO Abort Code
///
/// Defines the various reasons an SDO transfer can be aborted
#[derive(Clone, Copy, Debug, PartialEq, IntEnum)]
#[repr(u32)]
pub enum AbortCode {
    /// Toggle bit not alternated
    ToggleNotAlternated = 0x0503_0000,
    /// SDO protocol timed out
    SdoTimeout = 0x0504_0000,
    /// Client/server command specifier not valid or unknown
    InvalidCommandSpecifier = 0x0504_0001,
    /// Invalid block size (block mode only)
    InvalidBlockSize = 0x0504_0002,
    /// Invalid sequence number (block mode only)
    InvalidSequenceNumber = 0x0504_0003,
    /// CRC Error (block mode only )
    CrcError = 0x0504_0004,
    /// Out of memory
    OutOfMemory = 0x0504_0005,
    /// Unsupported access to an object
    UnsupportedAccess = 0x0601_0000,
    /// Attempt to read a write only object
    WriteOnly = 0x0601_0001,
    /// Attempt to write a read only object
    ReadOnly = 0x0601_0002,
    /// Object does not exist in the dictionary
    NoSuchObject = 0x0602_0000,
    /// Object cannot be mapped to the PDO
    UnnallowedPdo = 0x0604_0041,
    /// The number and length of objects would exceed PDO length
    PdoTooLong = 0x0604_0042,
    /// General parameter incompatibility
    IncompatibleParameter = 0x0604_0043,
    /// Access failed due to hardware error
    HardwareError = 0x0606_0000,
    /// Data type does not match, length of service parameter does not match
    DataTypeMismatch = 0x0607_0010,
    /// Data type does not match, length of service parameter too high
    DataTypeMismatchLengthHigh = 0x0607_0012,
    /// Data type does not match, length of service parameter too low
    DataTypeMismatchLengthLow = 0x0607_0013,
    /// Sub-index does not exist
    NoSuchSubIndex = 0x0609_0011,
    /// Invalid value for parameter (download only)
    InvalidValue = 0x0609_0030,
    /// Value of parameter too high (download only)
    ValueTooHigh = 0x0609_0031,
    /// Value of parameter too low (download only)
    ValueTooLow = 0x0609_0032,
    /// Resource isn't available
    ResourceNotAvailable = 0x060A_0023,
    /// General error
    GeneralError = 0x0800_0000,
    /// Data cannot be transferred or stored to the application
    CantStore = 0x0800_0020,
    /// Data cannot be transferred or stored to the application because of local control
    CantStoreLocalControl = 0x0800_0021,
    /// Data cannot be transferred or stored to the application because of the device state
    CantStoreDeviceState = 0x0800_0022,
    /// No object dictionary is present
    NoObjectDict = 0x0800_0023,
    /// No data available
    NoData = 0x0800_0024,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum ClientCommand {
    DownloadSegment = 0,
    InitiateDownload = 1,
    InitiateUpload = 2,
    ReqUploadSegment = 3,
    Abort = 4,
    BlockUpload = 5,
    BlockDownload = 6,
}

impl TryFrom<u8> for ClientCommand {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use ClientCommand::*;
        match value {
            0 => Ok(DownloadSegment),
            1 => Ok(InitiateDownload),
            2 => Ok(InitiateUpload),
            3 => Ok(ReqUploadSegment),
            4 => Ok(Abort),
            5 => Ok(BlockUpload),
            6 => Ok(BlockDownload),
            _ => Err(()),
        }
    }
}

/// Represents the CAN message used to send a segment during a block upload or download
#[derive(Clone, Copy, Debug)]
pub struct BlockSegment {
    /// Complete flag
    ///
    /// Indicates this is the last segment in the block transfer
    pub c: bool,
    /// The sequence number for the segment
    ///
    /// The sequence number starts at 1 on the first segment of a block, and increments on
    /// subsequent segment, up to a maximum of 127.
    pub seqnum: u8,
    /// The data bytes of this segment
    pub data: [u8; 7],
}

impl TryFrom<&[u8]> for BlockSegment {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(());
        }
        let c = (value[0] & (1 << 7)) != 0;
        let seqnum = value[0] & 0x7f;
        let data: [u8; 7] = value[1..8].try_into().unwrap();
        Ok(Self { c, seqnum, data })
    }
}

impl BlockSegment {
    /// Convert to the CAN message payload bytes
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0; 8];
        bytes[0] = (self.c as u8) << 7 | self.seqnum & 0x7f;
        bytes[1..8].copy_from_slice(&self.data);
        bytes
    }

    /// Create a CanMessage from the BlockSegment for transmission
    pub fn to_can_message(&self, id: CanId) -> CanMessage {
        CanMessage {
            data: self.to_bytes(),
            dlc: 8,
            rtr: false,
            id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum BlockDownloadClientSubcommand {
    InitiateDownload = 0,
    EndDownload = 1,
}

impl TryFrom<u8> for BlockDownloadClientSubcommand {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::InitiateDownload),
            1 => Ok(Self::EndDownload),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum BlockDownloadServerSubcommand {
    InitiateDownloadAck = 0,
    EndDownloadAck = 1,
    ConfirmBlock = 2,
}

impl TryFrom<u8> for BlockDownloadServerSubcommand {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::InitiateDownloadAck),
            1 => Ok(Self::EndDownloadAck),
            2 => Ok(Self::ConfirmBlock),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum BlockUploadClientSubcommand {
    InitiateUpload = 0,
    EndUpload = 1,
    ConfirmBlock = 2,
    StartUpload = 3,
}

impl TryFrom<u8> for BlockUploadClientSubcommand {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::InitiateUpload),
            1 => Ok(Self::EndUpload),
            2 => Ok(Self::ConfirmBlock),
            3 => Ok(Self::StartUpload),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum BlockUploadServerSubcommand {
    InitiateUpload = 0,
    EndUpload = 1,
}

impl TryFrom<u8> for BlockUploadServerSubcommand {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::InitiateUpload),
            1 => Ok(Self::EndUpload),
            _ => Err(()),
        }
    }
}

/// An SDO Request
///
/// This represents the possible request messages which can be send from client to server
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SdoRequest {
    /// Begin a download, writing data to an object on the server
    InitiateDownload {
        /// Number of unused bytes in data
        n: u8,
        /// Expedited
        e: bool,
        /// size valid
        s: bool,
        /// Object index
        index: u16,
        /// Object sub-index
        sub: u8,
        /// data (value on expedited, size when e=1 and s=1)
        data: [u8; 4],
    },
    /// Send a segment of data to the server
    DownloadSegment {
        /// Toggle flag
        t: bool,
        /// Number of unused bytes in data
        n: u8,
        /// When set, indicates there are no more segments to be sent
        c: bool,
        /// Segment data
        data: [u8; 7],
    },
    /// Begin an upload of data from an object on the server
    InitiateUpload {
        /// The requested object index
        index: u16,
        /// The requested sub object
        sub: u8,
    },
    /// Request the next segment in an upload
    ReqUploadSegment {
        /// Toggle flag
        t: bool,
    },
    /// Initiate a block download
    InitiateBlockDownload {
        /// Client CRC supported flag
        cc: bool,
        /// size flag
        s: bool,
        /// Index of object to download to
        index: u16,
        /// Sub object to download to
        sub: u8,
        /// If s=1, contains the number of bytes to be downloaded
        size: u32,
    },
    /// End a block download
    EndBlockDownload {
        /// Indicates the number of buytes in the last segment of the last block that do not contain
        /// data
        n: u8,
        /// CRC of the block (if supported by both client and server)
        crc: u16,
    },
    /// Initiate a block upload
    InitiateBlockUpload {
        /// Index of the object to upload
        index: u16,
        /// Sub index of the object to upload
        sub: u8,
        /// Number of segments per block
        blksize: u8,
        /// Protocol switch threshold
        ///
        /// Specifies when the server may switch the upload protocol to segmented or expedited
        /// pst = 0: Change of protocol not allowed
        /// pst > 0: If the size of the data is <= pst the server may switch the protocol
        pst: u8,
    },
    /// End a block upload
    EndBlockUpload,
    /// Request server to start sending upload block
    StartBlockUpload,
    /// Confirm reciept of a block from a block upload
    ConfirmBlock {
        /// The sequence number of the last successfully received block
        ackseq: u8,
        /// The number of segments to use for the next upload block
        blksize: u8,
    },

    /// Sent by client to abort an ongoing transaction
    Abort {
        /// The object index of the active transaction
        index: u16,
        /// The sub object of the active transaction
        sub: u8,
        /// The abort reason
        abort_code: u32,
    },
}

impl SdoRequest {
    /// Create an abort message
    pub fn abort(index: u16, sub: u8, abort_code: AbortCode) -> Self {
        SdoRequest::Abort {
            index,
            sub,
            abort_code: abort_code as u32,
        }
    }

    /// Create an initiate download request
    pub fn initiate_download(index: u16, sub: u8, size: Option<u32>) -> Self {
        let data = size.unwrap_or(0).to_le_bytes();

        SdoRequest::InitiateDownload {
            n: 0,
            e: false,
            s: size.is_some(),
            index,
            sub,
            data,
        }
    }

    /// Create an initiate block download request
    pub fn initiate_block_download(index: u16, sub: u8, crc_supported: bool, size: u32) -> Self {
        SdoRequest::InitiateBlockDownload {
            cc: crc_supported,
            s: true,
            index,
            sub,
            size,
        }
    }

    /// Create an end block download request
    ///
    /// # Arguments
    ///
    /// * `n` - Number of bytes in the last segment which do not contain valid data
    /// * `crc` - The CRC computed by client for the downloaded data
    pub fn end_block_download(n: u8, crc: u16) -> Self {
        SdoRequest::EndBlockDownload { n, crc }
    }

    /// Creat a `DownloadSegment` requests
    pub fn download_segment(toggle: bool, last_segment: bool, segment_data: &[u8]) -> Self {
        let mut data = [0; 7];
        data[0..segment_data.len()].copy_from_slice(segment_data);
        SdoRequest::DownloadSegment {
            t: toggle,
            n: 7 - segment_data.len() as u8,
            c: last_segment,
            data,
        }
    }

    /// Create an expedited download message
    pub fn expedited_download(index: u16, sub: u8, data: &[u8]) -> Self {
        let mut msg_data = [0; 4];
        msg_data[0..data.len()].copy_from_slice(data);

        SdoRequest::InitiateDownload {
            n: (4 - data.len()) as u8,
            e: true,
            s: true,
            index,
            sub,
            data: msg_data,
        }
    }

    /// Creata an `InitiateUpload` request
    pub fn initiate_upload(index: u16, sub: u8) -> Self {
        SdoRequest::InitiateUpload { index, sub }
    }

    /// Create a `ReqUploadSegment` request
    pub fn upload_segment_request(toggle: bool) -> Self {
        SdoRequest::ReqUploadSegment { t: toggle }
    }

    /// Convert the request to message payload bytes
    pub fn to_bytes(self) -> [u8; 8] {
        let mut payload = [0; 8];

        match self {
            SdoRequest::InitiateDownload {
                n,
                e,
                s,
                index,
                sub,
                data,
            } => {
                payload[0] = ((ClientCommand::InitiateDownload as u8) << 5)
                    | (n << 2)
                    | ((e as u8) << 1)
                    | s as u8;
                payload[1] = (index & 0xff) as u8;
                payload[2] = (index >> 8) as u8;
                payload[3] = sub;
                payload[4..8].copy_from_slice(&data);
            }
            SdoRequest::DownloadSegment { t, n, c, data } => {
                payload[0] = ((ClientCommand::DownloadSegment as u8) << 5)
                    | ((t as u8) << 4)
                    | ((n & 7) << 1)
                    | (c as u8);

                payload[1..8].copy_from_slice(&data);
            }
            SdoRequest::InitiateUpload { index, sub } => {
                payload[0] = (ClientCommand::InitiateUpload as u8) << 5;
                payload[1] = (index & 0xff) as u8;
                payload[2] = (index >> 8) as u8;
                payload[3] = sub;
            }
            SdoRequest::ReqUploadSegment { t } => {
                payload[0] = ((ClientCommand::ReqUploadSegment as u8) << 5) | ((t as u8) << 4);
            }
            SdoRequest::Abort {
                index,
                sub,
                abort_code,
            } => {
                payload[0] = (ClientCommand::Abort as u8) << 5;
                payload[1] = (index & 0xff) as u8;
                payload[2] = (index >> 8) as u8;
                payload[3] = sub;
                payload[4..8].copy_from_slice(&abort_code.to_le_bytes());
            }
            SdoRequest::InitiateBlockDownload {
                cc,
                s,
                index,
                sub,
                size,
            } => {
                payload[0] = ((ClientCommand::BlockDownload as u8) << 5)
                    | ((cc as u8) << 2)
                    | ((s as u8) << 1);
                payload[1] = (index & 0xff) as u8;
                payload[2] = (index >> 8) as u8;
                payload[3] = sub;
                payload[4..8].copy_from_slice(&size.to_le_bytes());
            }
            SdoRequest::EndBlockDownload { n, crc } => {
                payload[0] = ((ClientCommand::BlockDownload as u8) << 5)
                    | (n << 2)
                    | BlockDownloadClientSubcommand::EndDownload as u8;
                payload[1..3].copy_from_slice(&crc.to_le_bytes());
            }
            SdoRequest::InitiateBlockUpload {
                index: _,
                sub: _,
                blksize: _,
                pst: _,
            } => todo!(),
            SdoRequest::EndBlockUpload => todo!(),
            SdoRequest::StartBlockUpload => todo!(),
            SdoRequest::ConfirmBlock {
                ackseq: _,
                blksize: _,
            } => todo!(),
        }
        payload
    }

    /// Convert the request to a CanMessage using the provided COB ID
    pub fn to_can_message(self, id: CanId) -> CanMessage {
        let payload = self.to_bytes();
        CanMessage::new(id, &payload)
    }
}

impl TryFrom<&[u8]> for SdoRequest {
    type Error = AbortCode;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() < 8 {
            return Err(AbortCode::DataTypeMismatchLengthLow);
        }
        let ccs = value[0] >> 5;
        let ccs: ClientCommand = match ccs.try_into() {
            Ok(ccs) => ccs,
            Err(_) => return Err(AbortCode::InvalidCommandSpecifier),
        };

        match ccs {
            ClientCommand::DownloadSegment => {
                let t = (value[0] & (1 << 4)) != 0;
                let n = (value[0] >> 1) & 0x7;
                let c = (value[0] & (1 << 0)) != 0;
                let data = value[1..8].try_into().unwrap();
                Ok(SdoRequest::DownloadSegment { t, n, c, data })
            }
            ClientCommand::InitiateDownload => {
                let n = (value[0] >> 2) & 0x3;
                let e = (value[0] & (1 << 1)) != 0;
                let s = (value[0] & (1 << 0)) != 0;
                let index = value[1] as u16 | ((value[2] as u16) << 8);
                let sub = value[3];
                let data = value[4..8].try_into().unwrap();
                Ok(SdoRequest::InitiateDownload {
                    n,
                    e,
                    s,
                    index,
                    sub,
                    data,
                })
            }
            ClientCommand::InitiateUpload => {
                let index = value[1] as u16 | ((value[2] as u16) << 8);
                let sub = value[3];
                Ok(SdoRequest::InitiateUpload { index, sub })
            }
            ClientCommand::ReqUploadSegment => {
                let t = (((value[0]) >> 4) & 1) != 0;
                Ok(SdoRequest::ReqUploadSegment { t })
            }
            ClientCommand::Abort => {
                let index = value[1] as u16 | ((value[2] as u16) << 8);
                let sub = value[3];
                let abort_code = u32::from_le_bytes(value[4..8].try_into().unwrap());
                Ok(SdoRequest::Abort {
                    index,
                    sub,
                    abort_code,
                })
            }
            ClientCommand::BlockUpload => {
                let csc = value[0] & 3;
                let subcommand = match BlockUploadClientSubcommand::try_from(csc) {
                    Ok(subcommand) => subcommand,
                    Err(_) => return Err(AbortCode::InvalidCommandSpecifier),
                };
                match subcommand {
                    BlockUploadClientSubcommand::InitiateUpload => {
                        let index = value[1] as u16 | ((value[2] as u16) << 8);
                        let sub = value[3];
                        let blksize = value[4];
                        let pst = value[5];
                        Ok(SdoRequest::InitiateBlockUpload {
                            index,
                            sub,
                            blksize,
                            pst,
                        })
                    }
                    BlockUploadClientSubcommand::EndUpload => Ok(SdoRequest::EndBlockUpload),
                    BlockUploadClientSubcommand::ConfirmBlock => {
                        let ackseq = value[1];
                        let blksize = value[2];
                        Ok(SdoRequest::ConfirmBlock { ackseq, blksize })
                    }
                    BlockUploadClientSubcommand::StartUpload => Ok(SdoRequest::StartBlockUpload),
                }
            }
            ClientCommand::BlockDownload => {
                let csc = value[0] & 0x1;
                let subcommand = match BlockDownloadClientSubcommand::try_from(csc) {
                    Ok(subcommand) => subcommand,
                    Err(_) => return Err(AbortCode::InvalidCommandSpecifier),
                };
                match subcommand {
                    BlockDownloadClientSubcommand::InitiateDownload => {
                        let cc = (value[0] & (1 << 2)) != 0;
                        let s = (value[0] & (1 << 1)) != 0;
                        let index = value[1] as u16 | ((value[2] as u16) << 8);
                        let sub = value[3];
                        let size = u32::from_le_bytes(value[4..8].try_into().unwrap());
                        Ok(SdoRequest::InitiateBlockDownload {
                            cc,
                            s,
                            index,
                            sub,

                            size,
                        })
                    }
                    BlockDownloadClientSubcommand::EndDownload => {
                        let n = (value[0] >> 2) & 7;
                        let crc = u16::from_le_bytes(value[1..3].try_into().unwrap());
                        Ok(SdoRequest::EndBlockDownload { n, crc })
                    }
                }
            }
        }
    }
}

/// Represents a response from SDO server to client
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SdoResponse {
    /// Response to an [`SdoRequest::InitiateUpload`]
    ConfirmUpload {
        /// Number of unused bytes in data
        n: u8,
        /// Expedited flag
        e: bool,
        /// size flag
        s: bool,
        /// The index of the object being uploaded
        index: u16,
        /// The sub object being uploaded
        sub: u8,
        /// Value if e=1, or size if s=1
        data: [u8; 4],
    },
    /// Send an upload segment
    UploadSegment {
        /// The toggle bit
        t: bool,
        /// The number of unused bytes in data
        n: u8,
        /// Flag indicating this is the final segment
        c: bool,
        /// object data
        data: [u8; 7],
    },
    /// Response to a [`SdoRequest::InitiateDownload`]
    ConfirmDownload {
        /// The index of the object to be written to
        index: u16,
        /// The sub object to be written to
        sub: u8,
    },
    /// Response to a [`SdoRequest::DownloadSegment`]
    ConfirmDownloadSegment {
        /// Toggle flag
        t: bool,
    },
    /// Confirm a block download initiation
    ConfirmBlockDownload {
        /// Flag indicating server supports CRC generation
        sc: bool,
        /// Index of the object being downloaded
        index: u16,
        /// Sub index of the object being downloaded
        sub: u8,
        /// Number of segments for client to send in the next block
        blksize: u8,
    },
    /// Confirm completion of a block
    ConfirmBlock {
        /// Sequence number of the last segment successfully received by the server
        ackseq: u8,
        /// Number of segments for the client to send in the next block
        blksize: u8,
    },
    /// Confirm completion of a block download
    ConfirmBlockDownloadEnd,
    /// Confirm a block upload initiation
    ConfirmBlockUpload {
        /// Flag indicating server supports CRC on block transfer
        sc: bool,
        /// Size flag - indicates a valid size is stored in size field
        s: bool,
        /// Index of the object being uploaded
        index: u16,
        /// Sub index of the object being uploaded
        sub: u8,
        /// Size of the object to be uploaded
        size: u32,
    },
    /// Send by server to end block upload
    BlockUploadEnd {
        /// Indicates the number of bytes in the last segment which are not valid
        n: u8,
        /// The CRC of the block upload. Valid only if both server and client indicated support for
        /// CRC
        crc: u16,
    },
    /// Sent by server to abort an ongoing transaction
    Abort {
        /// Object index of the active transfer
        index: u16,
        /// Sub object of the active transfer
        sub: u8,
        /// Abort reason
        abort_code: u32,
    },
}

impl TryFrom<CanMessage> for SdoResponse {
    type Error = ();
    fn try_from(msg: CanMessage) -> Result<Self, Self::Error> {
        let scs = msg.data[0] >> 5;
        let command: ServerCommand = scs.try_into()?;
        match command {
            ServerCommand::SegmentUpload => {
                let t = (msg.data[0] & (1 << 4)) != 0;
                let n = (msg.data[0] >> 1) & 7;
                let c = (msg.data[0] & (1 << 0)) != 0;
                let data: [u8; 7] = msg.data[1..8].try_into().unwrap();

                Ok(SdoResponse::UploadSegment { t, n, c, data })
            }
            ServerCommand::SegmentDownload => {
                let t = (msg.data[0] & (1 << 4)) != 0;
                Ok(SdoResponse::ConfirmDownloadSegment { t })
            }
            ServerCommand::Upload => {
                let n = (msg.data[0] >> 2) & 0x3;
                let e = (msg.data[0] & (1 << 1)) != 0;
                let s = (msg.data[0] & (1 << 0)) != 0;
                let index = u16::from_le_bytes(msg.data[1..3].try_into().unwrap());
                let sub = msg.data[3];
                let data: [u8; 4] = msg.data[4..8].try_into().unwrap();
                Ok(SdoResponse::ConfirmUpload {
                    n,
                    e,
                    s,
                    index,
                    sub,
                    data,
                })
            }
            ServerCommand::Download => {
                let index = u16::from_le_bytes(msg.data[1..3].try_into().unwrap());
                let sub = msg.data[3];
                Ok(SdoResponse::ConfirmDownload { index, sub })
            }
            ServerCommand::BlockDownload => {
                match BlockDownloadServerSubcommand::try_from(msg.data[0] & 0x3)? {
                    BlockDownloadServerSubcommand::ConfirmBlock => {
                        let ackseq = msg.data[1];
                        let blksize = msg.data[2];
                        Ok(SdoResponse::ConfirmBlock { ackseq, blksize })
                    }
                    BlockDownloadServerSubcommand::InitiateDownloadAck => {
                        let sc = (msg.data[0] & (1 << 2)) != 0;
                        let index = u16::from_le_bytes(msg.data[1..3].try_into().unwrap());
                        let sub = msg.data[3];
                        let blksize = msg.data[4];
                        Ok(SdoResponse::ConfirmBlockDownload {
                            sc,
                            index,
                            sub,
                            blksize,
                        })
                    }
                    BlockDownloadServerSubcommand::EndDownloadAck => {
                        Ok(SdoResponse::ConfirmBlockDownloadEnd)
                    }
                }
            }
            ServerCommand::BlockUpload => {
                match BlockUploadServerSubcommand::try_from(msg.data[0] & 0x3)? {
                    BlockUploadServerSubcommand::InitiateUpload => {
                        let s = (msg.data[0] & (1 << 1)) != 0;
                        let sc = (msg.data[0] & (1 << 2)) != 0;
                        let index = u16::from_le_bytes(msg.data[1..3].try_into().unwrap());
                        let sub = msg.data[3];
                        let size = u32::from_le_bytes(msg.data[4..8].try_into().unwrap());
                        Ok(SdoResponse::ConfirmBlockUpload {
                            sc,
                            s,
                            index,
                            sub,
                            size,
                        })
                    }
                    BlockUploadServerSubcommand::EndUpload => {
                        let n = (msg.data[0] >> 2) & 7;
                        let crc = u16::from_le_bytes(msg.data[1..3].try_into().unwrap());
                        Ok(SdoResponse::BlockUploadEnd { n, crc })
                    }
                }
            }
            ServerCommand::Abort => {
                let index = u16::from_le_bytes(msg.data[1..3].try_into().unwrap());
                let sub = msg.data[3];
                let abort_code = u32::from_le_bytes(msg.data[4..8].try_into().unwrap());
                Ok(SdoResponse::Abort {
                    index,
                    sub,
                    abort_code,
                })
            }
        }
    }
}
impl SdoResponse {
    /// Create a `ConfirmUpload` response for an expedited upload
    pub fn expedited_upload(index: u16, sub: u8, data: &[u8]) -> SdoResponse {
        if data.len() > 4 {
            panic!("Cannot create expedited upload with more than 4 bytes");
        }

        let mut msg_data = [0; 4];
        msg_data[0..data.len()].copy_from_slice(data);

        let s;
        let n;
        // For 0 length uploads, set the size bit to 0, to indicate that this is an empty response.
        // It's not clear if this is CiA301 compatible, but it is how zencan does it!
        if data.is_empty() {
            s = false;
            n = 0;
        } else {
            s = true;
            n = 4 - data.len() as u8;
        }
        SdoResponse::ConfirmUpload {
            index,
            sub,
            e: true,
            s,
            n,
            data: msg_data,
        }
    }

    /// Create a `ConfirmUpload` response for a segmented upload
    pub fn upload_acknowledge(index: u16, sub: u8, size: Option<u32>) -> SdoResponse {
        SdoResponse::ConfirmUpload {
            n: 0,
            e: false,
            s: size.is_some(),
            index,
            sub,
            data: size.unwrap_or(0).to_le_bytes(),
        }
    }

    /// Create an `UploadSegment` response
    pub fn upload_segment(t: bool, c: bool, data: &[u8]) -> SdoResponse {
        let n = (7 - data.len()) as u8;
        let mut buf = [0; 7];
        buf[0..data.len()].copy_from_slice(data);
        SdoResponse::UploadSegment { t, n, c, data: buf }
    }

    /// Create a `ConfirmDownload` response
    pub fn download_acknowledge(index: u16, sub: u8) -> SdoResponse {
        SdoResponse::ConfirmDownload { index, sub }
    }

    /// Create a `ConfirmDownloadSegment` response
    pub fn download_segment_acknowledge(t: bool) -> SdoResponse {
        SdoResponse::ConfirmDownloadSegment { t }
    }

    /// Create a ConfirmBlockDownload response
    pub fn block_download_acknowledge(sc: bool, index: u16, sub: u8, blksize: u8) -> SdoResponse {
        SdoResponse::ConfirmBlockDownload {
            sc,
            index,
            sub,
            blksize,
        }
    }

    /// Create a ConfirmBlock response
    pub fn confirm_block(ackseq: u8, blksize: u8) -> SdoResponse {
        SdoResponse::ConfirmBlock { ackseq, blksize }
    }

    /// Create an abort response
    pub fn abort(index: u16, sub: u8, abort_code: AbortCode) -> SdoResponse {
        let abort_code = abort_code as u32;
        SdoResponse::Abort {
            index,
            sub,
            abort_code,
        }
    }

    /// Convert the response to a [CanMessage] using the provided COB ID
    pub fn to_can_message(self, id: CanId) -> CanMessage {
        let mut payload = [0; 8];

        match self {
            SdoResponse::ConfirmUpload {
                n,
                e,
                s,
                index,
                sub,
                data,
            } => {
                payload[0] = ((ServerCommand::Upload as u8) << 5)
                    | ((n & 0x3) << 2)
                    | ((e as u8) << 1)
                    | (s as u8);
                payload[1] = (index & 0xff) as u8;
                payload[2] = (index >> 8) as u8;
                payload[3] = sub;
                payload[4..8].copy_from_slice(&data);
            }
            SdoResponse::ConfirmDownload { index, sub } => {
                payload[0] = (ServerCommand::Download as u8) << 5;
                payload[1] = (index & 0xff) as u8;
                payload[2] = (index >> 8) as u8;
                payload[3] = sub;
            }
            SdoResponse::UploadSegment { t, n, c, data } => {
                payload[0] = ((ServerCommand::SegmentUpload as u8) << 5)
                    | ((t as u8) << 4)
                    | (n << 1)
                    | c as u8;
                payload[1..8].copy_from_slice(&data);
            }
            SdoResponse::ConfirmBlockDownload {
                sc,
                index,
                sub,
                blksize,
            } => {
                payload[0] = ((ServerCommand::BlockDownload as u8) << 5)
                    | ((sc as u8) << 2)
                    | (BlockDownloadServerSubcommand::InitiateDownloadAck as u8);
                payload[1] = (index & 0xff) as u8;
                payload[2] = (index >> 8) as u8;
                payload[3] = sub;
                payload[4] = blksize;
            }
            SdoResponse::ConfirmBlock { ackseq, blksize } => {
                payload[0] = ((ServerCommand::BlockDownload as u8) << 5)
                    | (BlockDownloadServerSubcommand::ConfirmBlock as u8);
                payload[1] = ackseq;
                payload[2] = blksize;
            }
            SdoResponse::ConfirmBlockDownloadEnd => {
                payload[0] = ((ServerCommand::BlockDownload as u8) << 5)
                    | (BlockDownloadServerSubcommand::EndDownloadAck as u8);
            }
            SdoResponse::ConfirmDownloadSegment { t } => {
                payload[0] = ((ServerCommand::SegmentDownload as u8) << 5) | ((t as u8) << 4);
            }
            SdoResponse::Abort {
                index,
                sub,
                abort_code,
            } => {
                payload[0] = (ServerCommand::Abort as u8) << 5;
                payload[1] = (index & 0xff) as u8;
                payload[2] = (index >> 8) as u8;
                payload[3] = sub;
                payload[4..8].copy_from_slice(&abort_code.to_le_bytes());
            }
            SdoResponse::ConfirmBlockUpload {
                sc: _,
                s: _,
                index: _,
                sub: _,
                size: _,
            } => todo!(),
            SdoResponse::BlockUploadEnd { n: _, crc: _ } => todo!(),
        }
        CanMessage::new(id, &payload)
    }
}
