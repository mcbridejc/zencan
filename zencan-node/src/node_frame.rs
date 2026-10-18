//! The frame types a node can be built for
use zencan_common::{
    can::{CanFdMessage, CanFrame, CanMessage, CLASSIC_MAX_LEN, FD_MAX_LEN},
    NodeId,
};

use crate::{object_dict::ObjectDictionary, pdo::Pdo, Callbacks, Node, NodeMbox, NodeState};

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
    /// `heapless::Vec<u8, 8>` for classic CAN, `heapless::Vec<u8, 64>` for CAN FD. The buffer holds
    /// the payload only, not a whole frame: the PDO already stores its COB-ID.
    type PdoPayload: PdoPayload;

    #[doc(hidden)]
    fn into_classic(self) -> Option<CanMessage>;

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
    fn pdo_init_defaults<'a>(pdo: &'a Pdo<'a, Self>);
}

/// A fixed capacity byte buffer holding the payload of one PDO
///
/// Implemented by `heapless::Vec<u8, N>`. This trait is sealed.
pub trait PdoPayload: Send + Default + sealed::Sealed {
    #[doc(hidden)]
    fn from_slice(data: &[u8]) -> Self;

    #[doc(hidden)]
    fn as_slice(&self) -> &[u8];
}

impl<const N: usize> sealed::Sealed for heapless::Vec<u8, N> {}

impl<const N: usize> PdoPayload for heapless::Vec<u8, N> {
    #[inline]
    fn from_slice(data: &[u8]) -> Self {
        // Unwrap: callers never store more than the frame's maximum payload
        heapless::Vec::from_slice(data).unwrap()
    }

    #[inline]
    fn as_slice(&self) -> &[u8] {
        self
    }
}

macro_rules! impl_node_frame {
    ($frame:ty, $max_len:expr) => {
        impl sealed::Sealed for $frame {}

        impl NodeFrame for $frame {
            type PdoPayload = heapless::Vec<u8, $max_len>;

            fn into_classic(self) -> Option<CanMessage> {
                CanMessage::try_from(self).ok()
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

            fn pdo_init_defaults<'a>(pdo: &'a Pdo<'a, Self>) {
                pdo.init_defaults_impl()
            }
        }
    };
}

impl_node_frame!(CanMessage, CLASSIC_MAX_LEN);
impl_node_frame!(CanFdMessage, FD_MAX_LEN);
