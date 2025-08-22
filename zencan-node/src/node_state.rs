//! Implements node state struct
use crate::object_dict::ObjectFlagSync;

use crate::pdo::Pdo;
use crate::storage::StorageContext;

/// A trait by which NodeState is accessed
pub trait NodeStateAccess: Sync + Send {
    /// Get the receive PDO objects
    fn get_rpdos(&self) -> &[Pdo];
    /// Get the transmit PDO objects
    fn get_tpdos(&self) -> &[Pdo];
    /// Get the PDO flag sync object
    fn get_pdo_sync(&self) -> &ObjectFlagSync;
    /// Get the storage context object
    fn storage_context(&self) -> &StorageContext;
}

/// The NodeState provides config-dependent storage to the [`Node`](crate::Node) object
///
/// The node state has to get instantiated (statically) by zencan-build, based on the device config
/// file. It is then provided to the node by the application when it is instantiated, and accessed
/// via the [`NodeStateAccess`] trait.
#[allow(missing_debug_implementations)]
pub struct NodeState<const N_RPDO: usize, const N_TPDO: usize> {
    rpdos: [Pdo; N_RPDO],
    tpdos: [Pdo; N_TPDO],
    pdo_sync: ObjectFlagSync,
    storage_context: StorageContext,
}

impl<const N_RPDO: usize, const N_TPDO: usize> Default for NodeState<N_RPDO, N_TPDO> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N_RPDO: usize, const N_TPDO: usize> NodeState<N_RPDO, N_TPDO> {
    /// Create a new NodeState object
    pub const fn new() -> Self {
        let rpdos = [const { Pdo::new() }; N_RPDO];
        let tpdos = [const { Pdo::new() }; N_TPDO];
        let pdo_sync = ObjectFlagSync::new();
        let storage_context = StorageContext::new();
        Self {
            rpdos,
            tpdos,
            pdo_sync,
            storage_context,
        }
    }

    /// Access the RPDOs as a const function
    pub const fn rpdos(&'static self) -> &'static [Pdo] {
        &self.rpdos
    }

    /// Access the TPDOs as a const function
    pub const fn tpdos(&'static self) -> &'static [Pdo] {
        &self.tpdos
    }

    /// Access the pdo_sync as a const function
    ///
    /// This is required so that it can be shared with the objects in generated code
    pub const fn pdo_sync(&'static self) -> &'static ObjectFlagSync {
        &self.pdo_sync
    }

    /// Access the storage_context as a const function
    pub const fn storage_context(&'static self) -> &'static StorageContext {
        &self.storage_context
    }
}

impl<const N_RPDO: usize, const N_TPDO: usize> NodeStateAccess for NodeState<N_RPDO, N_TPDO> {
    fn get_rpdos(&self) -> &[Pdo] {
        &self.rpdos
    }

    fn get_tpdos(&self) -> &[Pdo] {
        &self.tpdos
    }

    fn get_pdo_sync(&self) -> &ObjectFlagSync {
        &self.pdo_sync
    }

    fn storage_context(&self) -> &StorageContext {
        &self.storage_context
    }
}
