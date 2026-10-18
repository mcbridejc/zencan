//! A frame abstraction shared by classic CAN and CAN FD
//!
//! Two concrete frame types exist: [`CanMessage`] for classic CAN (up to 8 data bytes, remote
//! frames allowed) and [`CanFdMessage`] for CAN FD (up to 64 data bytes, no remote frames). The
//! [`CanFrame`] trait gives generic code uniform access to both, so that a node only pays for the
//! frame type it actually uses: a type which is never instantiated costs neither RAM nor flash.

use snafu::Snafu;

use super::{CanId, CanMessage};

/// The maximum payload length of a classic CAN frame, in bytes
pub const CLASSIC_MAX_LEN: usize = 8;
/// The maximum payload length of a CAN FD frame, in bytes
pub const FD_MAX_LEN: usize = 64;

/// Round a payload length up to the next length which can be encoded in a CAN FD frame
///
/// CAN FD frames can only carry 0 to 8, 12, 16, 20, 24, 32, 48 or 64 data bytes. A payload of any
/// other length has to be padded up to the next legal length before transmission.
///
/// Lengths above [`FD_MAX_LEN`] cannot be represented; they saturate to [`FD_MAX_LEN`].
pub const fn fd_padded_len(len: usize) -> usize {
    match len {
        0..=8 => len,
        9..=12 => 12,
        13..=16 => 16,
        17..=20 => 20,
        21..=24 => 24,
        25..=32 => 32,
        33..=48 => 48,
        _ => FD_MAX_LEN,
    }
}

/// Error returned when a frame cannot be constructed
#[derive(Clone, Copy, Debug, PartialEq, Eq, Snafu)]
pub enum FrameError {
    /// The payload is longer than the frame format allows
    #[snafu(display("payload of {len} bytes exceeds the {max} byte limit of the frame format"))]
    TooLong {
        /// The requested payload length
        len: usize,
        /// The maximum payload length of the frame format
        max: usize,
    },
    /// The frame format has no remote (RTR) frames
    #[snafu(display("the frame format does not support remote frames"))]
    RtrNotSupported,
}

/// Common access to the CAN frame types
///
/// Implemented by [`CanMessage`] (classic CAN) and [`CanFdMessage`] (CAN FD). Zencan's transport
/// path (mailbox, PDOs, CAN traits) is generic over this trait, with [`CanMessage`] as the default
/// type parameter everywhere.
pub trait CanFrame: Copy + Send + Sized + core::fmt::Debug {
    /// The maximum payload length in bytes
    const MAX_LEN: usize;
    /// True if the frame format supports remote transmission requests
    const SUPPORTS_RTR: bool;
    /// True if this is a CAN FD frame type
    const IS_FD: bool;

    /// Create a data frame carrying `data`
    ///
    /// Fails if `data` is longer than [`Self::MAX_LEN`].
    fn try_new(id: CanId, data: &[u8]) -> Result<Self, FrameError>;

    /// Create a data frame with a payload of `len` zero bytes
    ///
    /// The payload can then be filled in place through [`data_mut`](Self::data_mut), without
    /// building it in a temporary buffer first. Fails if `len` exceeds [`Self::MAX_LEN`].
    fn try_new_zeroed(id: CanId, len: usize) -> Result<Self, FrameError>;

    /// Create a remote transmission request frame
    ///
    /// Fails with [`FrameError::RtrNotSupported`] for formats without remote frames (CAN FD).
    fn try_new_rtr(id: CanId) -> Result<Self, FrameError>;

    /// The ID of the frame
    fn id(&self) -> CanId;

    /// The payload of the frame, `len()` bytes long
    fn data(&self) -> &[u8];

    /// Mutable access to the payload of the frame, `len()` bytes long
    fn data_mut(&mut self) -> &mut [u8];

    /// The payload length in bytes
    fn len(&self) -> usize;

    /// Returns true if the frame has no payload
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if this frame is a remote transmission request
    fn is_rtr(&self) -> bool;

    /// The number of data bytes actually transmitted on the bus
    ///
    /// This equals `len()` for classic CAN. For CAN FD it is `len()` rounded up to the next legal
    /// CAN FD length (see [`fd_padded_len`]).
    fn on_wire_len(&self) -> usize;

    /// The payload followed by zero padding, `on_wire_len()` bytes long
    ///
    /// This is what a driver should hand to the CAN controller for transmission.
    fn padded_data(&self) -> &[u8];
}

impl CanFrame for CanMessage {
    const MAX_LEN: usize = CLASSIC_MAX_LEN;
    const SUPPORTS_RTR: bool = true;
    const IS_FD: bool = false;

    fn try_new(id: CanId, data: &[u8]) -> Result<Self, FrameError> {
        let mut msg = Self::try_new_zeroed(id, data.len())?;
        msg.data[..data.len()].copy_from_slice(data);
        Ok(msg)
    }

    fn try_new_zeroed(id: CanId, len: usize) -> Result<Self, FrameError> {
        if len > CLASSIC_MAX_LEN {
            return Err(FrameError::TooLong {
                len,
                max: CLASSIC_MAX_LEN,
            });
        }
        Ok(Self {
            id,
            dlc: len as u8,
            ..Default::default()
        })
    }

    fn try_new_rtr(id: CanId) -> Result<Self, FrameError> {
        Ok(Self::new_rtr(id))
    }

    fn id(&self) -> CanId {
        self.id
    }

    fn data(&self) -> &[u8] {
        &self.data[..self.dlc as usize]
    }

    fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data[..self.dlc as usize]
    }

    fn len(&self) -> usize {
        self.dlc as usize
    }

    fn is_rtr(&self) -> bool {
        self.rtr
    }

    fn on_wire_len(&self) -> usize {
        self.dlc as usize
    }

    fn padded_data(&self) -> &[u8] {
        CanFrame::data(self)
    }
}

/// A CAN FD data frame
///
/// Carries up to [`FD_MAX_LEN`] bytes of payload. CAN FD has no remote frames.
///
/// The payload bytes beyond [`len`](CanFrame::len) are always zero, so
/// [`padded_data`](CanFrame::padded_data) yields the frame correctly padded to its on-wire length.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CanFdMessage {
    data: [u8; FD_MAX_LEN],
    len: u8,
    id: CanId,
    brs: bool,
    esi: bool,
}

impl Default for CanFdMessage {
    fn default() -> Self {
        Self::zeroed(CanId::Std(0), 0)
    }
}

impl core::fmt::Debug for CanFdMessage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CanFdMessage")
            .field("id", &self.id)
            .field("data", &self.data())
            .field("brs", &self.brs)
            .field("esi", &self.esi)
            .finish()
    }
}

impl CanFdMessage {
    /// Build a frame of `len` zero bytes; `len` must already be validated
    const fn zeroed(id: CanId, len: usize) -> Self {
        Self {
            data: [0; FD_MAX_LEN],
            len: len as u8,
            id,
            brs: true,
            esi: false,
        }
    }

    /// Create a new CAN FD message
    ///
    /// The frame requests bit rate switching; see [`with_brs`](Self::with_brs).
    ///
    /// # Panics
    ///
    /// Panics if `data` is longer than [`FD_MAX_LEN`]. Use [`CanFrame::try_new`] for a fallible
    /// version.
    pub fn new(id: CanId, data: &[u8]) -> Self {
        match <Self as CanFrame>::try_new(id, data) {
            Ok(msg) => msg,
            Err(_) => panic!("Data length exceeds maximum size of {} bytes", FD_MAX_LEN),
        }
    }

    /// Get the id of the message
    pub fn id(&self) -> CanId {
        self.id
    }

    /// Get a slice containing the data payload
    pub fn data(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }

    /// Returns true if the data phase is to be (or was) transmitted at the data bit rate
    ///
    /// Frames created by zencan request bit rate switching. Whether the data phase is actually
    /// switched depends on the controller configuration.
    pub fn brs(&self) -> bool {
        self.brs
    }

    /// Set the bit rate switch flag
    pub fn set_brs(&mut self, brs: bool) {
        self.brs = brs;
    }

    /// Return the message with the bit rate switch flag set to `brs`
    pub fn with_brs(mut self, brs: bool) -> Self {
        self.brs = brs;
        self
    }

    /// The error state indicator of a received frame
    ///
    /// Set when the transmitting node was error passive. The transmitting controller generates
    /// this flag itself; drivers report it on reception and ignore it on transmission.
    pub fn esi(&self) -> bool {
        self.esi
    }

    /// Set the error state indicator, typically by a driver on reception
    pub fn set_esi(&mut self, esi: bool) {
        self.esi = esi;
    }
}

impl CanFrame for CanFdMessage {
    const MAX_LEN: usize = FD_MAX_LEN;
    const SUPPORTS_RTR: bool = false;
    const IS_FD: bool = true;

    fn try_new(id: CanId, data: &[u8]) -> Result<Self, FrameError> {
        let mut msg = Self::try_new_zeroed(id, data.len())?;
        msg.data[..data.len()].copy_from_slice(data);
        Ok(msg)
    }

    fn try_new_zeroed(id: CanId, len: usize) -> Result<Self, FrameError> {
        if len > FD_MAX_LEN {
            return Err(FrameError::TooLong {
                len,
                max: FD_MAX_LEN,
            });
        }
        Ok(Self::zeroed(id, len))
    }

    fn try_new_rtr(_id: CanId) -> Result<Self, FrameError> {
        Err(FrameError::RtrNotSupported)
    }

    fn id(&self) -> CanId {
        self.id
    }

    fn data(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }

    fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data[..self.len as usize]
    }

    fn len(&self) -> usize {
        self.len as usize
    }

    fn is_rtr(&self) -> bool {
        false
    }

    fn on_wire_len(&self) -> usize {
        fd_padded_len(self.len as usize)
    }

    fn padded_data(&self) -> &[u8] {
        &self.data[..self.on_wire_len()]
    }
}

/// Widen a classic frame to a CAN FD frame
///
/// The payload and ID are preserved and bit rate switching is requested. Remote frames have no CAN
/// FD equivalent and are rejected with [`FrameError::RtrNotSupported`].
impl TryFrom<CanMessage> for CanFdMessage {
    type Error = FrameError;

    fn try_from(msg: CanMessage) -> Result<Self, Self::Error> {
        if msg.rtr {
            return Err(FrameError::RtrNotSupported);
        }
        <Self as CanFrame>::try_new(msg.id, msg.data())
    }
}

/// Narrow a CAN FD frame to a classic frame
///
/// Fails with [`FrameError::TooLong`] when the payload exceeds [`CLASSIC_MAX_LEN`].
impl TryFrom<CanFdMessage> for CanMessage {
    type Error = FrameError;

    fn try_from(msg: CanFdMessage) -> Result<Self, Self::Error> {
        <Self as CanFrame>::try_new(msg.id, msg.data())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legal CAN FD payload lengths, indexed by DLC
    const FD_DLC_TO_LEN: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

    #[test]
    fn test_fd_padded_len_table() {
        for len in 0..=FD_MAX_LEN {
            // The smallest legal length which can hold `len` bytes
            let expected = *FD_DLC_TO_LEN.iter().find(|&&l| l >= len).unwrap();
            assert_eq!(expected, fd_padded_len(len), "len {len}");
        }
    }

    #[test]
    fn test_classic_message_layout_unchanged() {
        // The classic frame keeps its exact public representation
        let msg = CanMessage {
            data: [1, 2, 3, 4, 5, 6, 7, 8],
            dlc: 8,
            rtr: false,
            id: CanId::Std(0x123),
        };
        assert_eq!(
            msg,
            CanMessage::new(CanId::Std(0x123), &[1, 2, 3, 4, 5, 6, 7, 8])
        );
        assert_eq!(20, core::mem::size_of::<CanMessage>());
    }

    #[test]
    fn test_classic_frame_trait() {
        let id = CanId::Std(0x181);
        let msg = <CanMessage as CanFrame>::try_new(id, &[1, 2, 3]).unwrap();
        assert_eq!(CanMessage::new(id, &[1, 2, 3]), msg);
        assert_eq!(3, CanFrame::len(&msg));
        assert_eq!(3, msg.on_wire_len());
        assert_eq!(&[1, 2, 3], msg.padded_data());
        assert_eq!(
            Err(FrameError::TooLong { len: 9, max: 8 }),
            <CanMessage as CanFrame>::try_new(id, &[0; 9])
        );
        assert!(<CanMessage as CanFrame>::try_new_rtr(id).unwrap().is_rtr());

        let mut zeroed = <CanMessage as CanFrame>::try_new_zeroed(id, 2).unwrap();
        zeroed.data_mut().copy_from_slice(&[5, 6]);
        assert_eq!(CanMessage::new(id, &[5, 6]), zeroed);
    }

    #[test]
    fn test_fd_frame_lengths() {
        let id = CanId::Extended(0x1234);
        for len in 0..=FD_MAX_LEN {
            let data: [u8; FD_MAX_LEN] = core::array::from_fn(|i| i as u8 + 1);
            let msg = CanFdMessage::try_new(id, &data[..len]).unwrap();
            assert_eq!(len, CanFrame::len(&msg));
            assert_eq!(&data[..len], CanFrame::data(&msg));
            assert_eq!(fd_padded_len(len), msg.on_wire_len());
            let padded = msg.padded_data();
            assert_eq!(fd_padded_len(len), padded.len());
            assert_eq!(&data[..len], &padded[..len]);
            assert!(padded[len..].iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn test_fd_rejects_oversize() {
        let id = CanId::Std(0x200);
        assert_eq!(
            Err(FrameError::TooLong { len: 65, max: 64 }),
            CanFdMessage::try_new(id, &[0; 65])
        );
        assert_eq!(
            Err(FrameError::TooLong { len: 65, max: 64 }),
            CanFdMessage::try_new_zeroed(id, 65)
        );
    }

    #[test]
    #[should_panic]
    fn test_fd_new_panics_on_oversize() {
        CanFdMessage::new(CanId::Std(0x200), &[0; 65]);
    }

    #[test]
    fn test_fd_rejects_rtr() {
        assert_eq!(
            Err(FrameError::RtrNotSupported),
            CanFdMessage::try_new_rtr(CanId::Std(0x200))
        );
        let rtr = CanMessage::new_rtr(CanId::Std(0x200));
        assert_eq!(
            Err(FrameError::RtrNotSupported),
            CanFdMessage::try_from(rtr)
        );
    }

    #[test]
    fn test_fd_zeroed_fill_in_place() {
        let id = CanId::Std(0x300);
        let mut msg = CanFdMessage::try_new_zeroed(id, 10).unwrap();
        assert_eq!(&[0; 10], msg.data());
        msg.data_mut().copy_from_slice(&[9; 10]);
        assert_eq!(&[9; 10], msg.data());
        // Padding to 12 bytes stays zero
        assert_eq!(&[9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 0, 0], msg.padded_data());
    }

    #[test]
    fn test_classic_to_fd_conversion() {
        let classic = CanMessage::new(CanId::Std(0x581), &[0x43, 0, 0x10, 0, 1, 2, 3, 4]);
        let fd = CanFdMessage::try_from(classic).unwrap();
        assert_eq!(classic.id(), fd.id());
        assert_eq!(classic.data(), fd.data());
        assert_eq!(8, fd.on_wire_len());
        assert!(fd.brs());
        assert!(!fd.esi());
        // And back again
        assert_eq!(classic, CanMessage::try_from(fd).unwrap());

        let empty = CanFdMessage::try_from(CanMessage::new(CanId::Std(0x80), &[])).unwrap();
        assert!(CanFrame::is_empty(&empty));
    }

    #[test]
    fn test_fd_to_classic_rejects_long_frames() {
        let fd = CanFdMessage::new(CanId::Std(0x181), &[0; 9]);
        assert_eq!(
            Err(FrameError::TooLong { len: 9, max: 8 }),
            CanMessage::try_from(fd)
        );
    }

    #[test]
    fn test_fd_flags() {
        let mut msg = CanFdMessage::new(CanId::Std(1), &[1]).with_brs(false);
        assert!(!msg.brs());
        msg.set_brs(true);
        msg.set_esi(true);
        assert!(msg.brs());
        assert!(msg.esi());

        /// The format constants of a frame type: (IS_FD, SUPPORTS_RTR, MAX_LEN)
        fn format<F: CanFrame>() -> (bool, bool, usize) {
            (F::IS_FD, F::SUPPORTS_RTR, F::MAX_LEN)
        }
        assert_eq!((true, false, 64), format::<CanFdMessage>());
        assert_eq!((false, true, 8), format::<CanMessage>());
    }
}
