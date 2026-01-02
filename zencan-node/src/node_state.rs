//! Implements node state struct
use zencan_common::nmt::NmtState;
use zencan_common::AtomicCell;

use crate::object_dict::ObjectFlagSync;

use crate::pdo::Pdo;
use crate::storage::StorageContext;

pub trait NmtStateAccess: Send + Sync {
    fn nmt_state(&self) -> NmtState;
}

impl NmtStateAccess for AtomicCell<NmtState> {
    fn nmt_state(&self) -> NmtState {
        self.load()
    }
}

/// The NodeState provides config-dependent storage to the [`Node`](crate::Node) object
///
/// The node state has to get instantiated (statically) by zencan-build, based on the device config
/// via the [`NodeStateAccess`] trait.
/// file. It is then provided to the node by the application when it is instantiated, and accessed
#[allow(missing_debug_implementations)]
pub struct NodeState<'a> {
    /// Pdo control objects for receive PDOs
    rpdos: &'a [Pdo<'a>],
    /// Pdo control objects for transmit PDOs
    tpdos: &'a [Pdo<'a>],
    /// A global flag used by all objects to synchronize their event flag A/B swapping
    object_flag_sync: ObjectFlagSync,
    /// State shared between the [`StorageCommandObject`](crate::storage::StorageCommandObject) and
    /// [`Node`](crate::node::Node) for indicating when a store objects command has been recieved.
    storage_context: StorageContext,
    /// Global storage for the NMT state
    nmt_state: AtomicCell<NmtState>,
}

impl NmtStateAccess for NodeState<'_> {
    fn nmt_state(&self) -> NmtState {
        self.nmt_state.load()
    }
}

impl<'a> NodeState<'a> {
    /// Create a new NodeState object
    pub const fn new(rpdos: &'a [Pdo<'a>], tpdos: &'a [Pdo<'a>]) -> Self {
        let object_flag_sync = ObjectFlagSync::new();
        let storage_context = StorageContext::new();

        Self {
            rpdos,
            tpdos,
            object_flag_sync,
            storage_context,
            nmt_state: AtomicCell::new(NmtState::Bootup),
        }
    }

    /// Access the RPDOs as a const function
    pub const fn rpdos(&self) -> &'a [Pdo<'a>] {
        self.rpdos
    }

    /// Access the TPDOs as a const function
    pub const fn tpdos(&self) -> &'a [Pdo<'a>] {
        self.tpdos
    }

    /// Access the pdo_sync as a const function
    ///
    /// This is required so that it can be shared with the objects in generated code
    pub const fn object_flag_sync(&'a self) -> &'a ObjectFlagSync {
        &self.object_flag_sync
    }

    /// Access the storage_context as a const function
    pub const fn storage_context(&'a self) -> &'a StorageContext {
        &self.storage_context
    }

    /// Set the NMT state
    ///
    /// This method is intended only for the `Node` object to update the global node nmt state
    pub(crate) fn set_nmt_state(&self, nmt_state: NmtState) {
        self.nmt_state.store(nmt_state);
    }
}
