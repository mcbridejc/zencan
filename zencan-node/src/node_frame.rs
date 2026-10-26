//! The frame types a node can be built for
use core::ops::Deref;

use zencan_common::{
    can::{CanFdMessage, CanFrame, CanMessage, CLASSIC_MAX_LEN, FD_MAX_LEN},
    NodeId,
};

use crate::{
    object_dict::ObjectDictionary,
    pdo::{Pdo, PdoData},
    Callbacks, Node, NodeMbox, NodeState,
};

mod sealed {
    pub trait Sealed {}
}

/// A CAN frame type a node can run on: [`CanMessage`] (classic CAN) or [`CanFdMessage`] (CAN FD)
///
/// This trait is sealed. Besides carrying the frame-dependent PDO storage type, it lets the node's
/// frame-dependent code be compiled inside this crate, once per frame type, rather than in the
/// application crate. An application built for classic CAN therefore links exactly the code it
/// linked before CAN FD support existed, and pays nothing for it.
pub trait NodeFrame: CanFrame + TryFrom<CanMessage> + sealed::Sealed + 'static {
    /// Storage for the payload of one buffered PDO, with room for [`CanFrame::MAX_LEN`] bytes
    ///
    /// [`PdoBuffer<8>`](PdoBuffer) for classic CAN, [`PdoBuffer<64>`](PdoBuffer) for CAN FD. The
    /// buffer holds the payload only, not a whole frame: the PDO already stores its COB-ID.
    type PdoPayload: PdoPayload;

    /// Convert to a classic frame, keeping only the first 8 bytes of a longer CAN FD frame
    #[doc(hidden)]
    fn into_classic(self) -> CanMessage;

    #[doc(hidden)]
    fn node_new<'a>(
        node_id: NodeId,
        callbacks: Callbacks<'a>,
        mbox: &'static NodeMbox<Self>,
        state: &'static NodeState<'static, Self>,
        od: &'static dyn ObjectDictionary,
    ) -> Node<'a, Self>;

    #[doc(hidden)]
    fn node_process(node: &mut Node<'_, Self>, now_us: u64) -> bool;

    #[doc(hidden)]
    fn mbox_store_message(mbox: &NodeMbox<Self>, msg: Self) -> Result<(), Self>;

    #[doc(hidden)]
    fn mbox_next_transmit_message(mbox: &NodeMbox<Self>) -> Option<Self>;

    #[doc(hidden)]
    fn pdo_data_default<'a>() -> PdoData<'a, Self>;

    #[doc(hidden)]
    fn pdo_init_defaults<'a>(pdo: &'a Pdo<'a, Self>);
}

/// A fixed capacity byte buffer holding the payload of one PDO
///
/// Implemented by [`PdoBuffer`]. This trait is sealed.
pub trait PdoPayload: Send + Default + Deref<Target = [u8]> + sealed::Sealed {
    /// Create a payload holding `data`, truncated to the buffer capacity
    #[doc(hidden)]
    fn from_slice(data: &[u8]) -> Self;

    /// Append `len` zero bytes and return them for filling, or `None` if they do not fit
    #[doc(hidden)]
    fn extend_zeroed(&mut self, len: usize) -> Option<&mut [u8]>;
}

/// The payload of one PDO: up to `N` bytes
///
/// Dereferences to the payload bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdoBuffer<const N: usize> {
    len: u8,
    // Bytes beyond `len` are always zero
    data: [u8; N],
}

impl<const N: usize> Default for PdoBuffer<N> {
    fn default() -> Self {
        Self {
            len: 0,
            data: [0; N],
        }
    }
}

impl<const N: usize> Deref for PdoBuffer<N> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }
}

impl<const N: usize> sealed::Sealed for PdoBuffer<N> {}

impl<const N: usize> PdoPayload for PdoBuffer<N> {
    #[inline]
    fn from_slice(data: &[u8]) -> Self {
        // A frame never carries more than the buffer capacity, so this never truncates
        let len = data.len().min(N);
        let mut buffer = Self::default();
        buffer.data[..len].copy_from_slice(&data[..len]);
        buffer.len = len as u8;
        buffer
    }

    #[inline]
    fn extend_zeroed(&mut self, len: usize) -> Option<&mut [u8]> {
        let start = self.len as usize;
        let end = start.checked_add(len).filter(|&end| end <= N)?;
        self.len = end as u8;
        Some(&mut self.data[start..end])
    }
}

macro_rules! impl_node_frame {
    ($frame:ty, $max_len:expr) => {
        impl sealed::Sealed for $frame {}

        impl NodeFrame for $frame {
            type PdoPayload = PdoBuffer<$max_len>;

            fn into_classic(self) -> CanMessage {
                match CanMessage::try_from(self) {
                    Ok(msg) => msg,
                    // A CAN FD peer may pad a frame beyond its significant bytes, which never
                    // exceed 8 for the messages converted here. Unreachable for classic CAN.
                    Err(_) => CanMessage::new(self.id(), &self.data()[..CLASSIC_MAX_LEN]),
                }
            }

            fn node_new<'a>(
                node_id: NodeId,
                callbacks: Callbacks<'a>,
                mbox: &'static NodeMbox<Self>,
                state: &'static NodeState<'static, Self>,
                od: &'static dyn ObjectDictionary,
            ) -> Node<'a, Self> {
                Node::new_impl(node_id, callbacks, mbox, state, od)
            }

            fn node_process(node: &mut Node<'_, Self>, now_us: u64) -> bool {
                node.process_impl(now_us)
            }

            fn mbox_store_message(mbox: &NodeMbox<Self>, msg: Self) -> Result<(), Self> {
                mbox.store_message_impl(msg)
            }

            fn mbox_next_transmit_message(mbox: &NodeMbox<Self>) -> Option<Self> {
                mbox.next_transmit_message_impl()
            }

            fn pdo_data_default<'a>() -> PdoData<'a, Self> {
                PdoData::default_impl()
            }

            fn pdo_init_defaults<'a>(pdo: &'a Pdo<'a, Self>) {
                pdo.init_defaults_impl()
            }
        }
    };
}

impl_node_frame!(CanMessage, CLASSIC_MAX_LEN);
impl_node_frame!(CanFdMessage, FD_MAX_LEN);
