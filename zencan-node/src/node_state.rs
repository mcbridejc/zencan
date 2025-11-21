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
    fn object_flag_sync(&self) -> &ObjectFlagSync;
    /// Get the storage context object
    fn storage_context(&self) -> &StorageContext;
}

/// The NodeState provides config-dependent storage to the [`Node`](crate::Node) object
///
/// The node state has to get instantiated (statically) by zencan-build, based on the device config
/// via the [`NodeStateAccess`] trait.
/// file. It is then provided to the node by the application when it is instantiated, and accessed
#[allow(missing_debug_implementations)]
pub struct NodeState<'a> {
    /// Pdo control objects for receive PDOs
    rpdos: &'a [Pdo],
    /// Pdo control objects for transmit PDOs
    tpdos: &'a [Pdo],
    /// A global flag used by all objects to synchronize their event flag A/B swapping
    object_flag_sync: ObjectFlagSync,
    /// State shared between the [`StorageCommandObject`](crate::storage::StorageCommandObject) and
    /// [`Node`](crate::node::Node) for indicating when a store objects command has been recieved.
    storage_context: StorageContext,
}

impl<'a> NodeState<'a> {
    /// Create a new NodeState object
    pub const fn new(rpdos: &'a [Pdo], tpdos: &'a [Pdo]) -> Self {
        let object_flag_sync = ObjectFlagSync::new();
        let storage_context = StorageContext::new();

        Self {
            rpdos,
            tpdos,
            object_flag_sync,
            storage_context,
        }
    }

    /// Access the RPDOs as a const function
    pub const fn rpdos(&self) -> &'a [Pdo] {
        self.rpdos
    }

    /// Access the TPDOs as a const function
    pub const fn tpdos(&self) -> &'a [Pdo] {
        self.tpdos
    }

    /// Access the pdo_sync as a const function
    ///
    /// This is required so that it can be shared with the objects in generated code
    pub const fn object_flag_sync(&'static self) -> &'static ObjectFlagSync {
        &self.object_flag_sync
    }

    /// Access the storage_context as a const function
    pub const fn storage_context(&'static self) -> &'static StorageContext {
        &self.storage_context
    }
}

impl NodeStateAccess for NodeState<'_> {
    fn get_rpdos(&self) -> &[Pdo] {
        self.rpdos
    }

    fn get_tpdos(&self) -> &[Pdo] {
        self.tpdos
    }

    fn object_flag_sync(&self) -> &ObjectFlagSync {
        &self.object_flag_sync
    }

    fn storage_context(&self) -> &StorageContext {
        &self.storage_context
    }
}
