mod sdo_comms;
mod sdo_server;

pub(crate) use sdo_comms::SdoComms;
pub(crate) use sdo_server::SdoServer;

/// Default size for SDO data buffer
///
/// Enough for 127 segments of 7 bytes each, which is the maximum size of a block transfer
pub const SDO_BUFFER_SIZE: usize = 889;

/// The SDO buffer size for a dictionary whose largest sub-object is `largest_object` bytes
///
/// Block transfer needs the full [`SDO_BUFFER_SIZE`]. Segmented transfer only needs to hold the
/// largest sub-object, so that an object which does not support partial writes can be written in
/// one piece, rounded up to whole 7-byte segments. A domain has no fixed size, so zencan-build
/// uses the full [`SDO_BUFFER_SIZE`] for a dictionary with a domain instead.
pub const fn sdo_buffer_size(largest_object: usize) -> usize {
    if cfg!(feature = "sdo-block") {
        SDO_BUFFER_SIZE
    } else if largest_object < 7 {
        7
    } else {
        largest_object.div_ceil(7) * 7
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "sdo-block")]
    fn sdo_buffer_fits_a_full_block() {
        assert_eq!(sdo_buffer_size(4), SDO_BUFFER_SIZE);
    }

    #[test]
    #[cfg(not(feature = "sdo-block"))]
    fn sdo_buffer_fits_the_largest_object_in_whole_segments() {
        assert_eq!(sdo_buffer_size(0), 7);
        assert_eq!(sdo_buffer_size(4), 7);
        assert_eq!(sdo_buffer_size(7), 7);
        assert_eq!(sdo_buffer_size(14), 14);
        assert_eq!(sdo_buffer_size(15), 21);
    }
}
