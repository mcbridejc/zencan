use core::cell::UnsafeCell;

use critical_section::Mutex;
use zencan_common::AtomicCell;

/// A struct used for synchronizing the A/B event flags of all objects, which are used for
/// triggering PDO events
#[derive(Debug)]
pub struct ObjectFlagSync {
    inner: Mutex<UnsafeCell<ObjectFlagsInner>>,
}

impl Default for ObjectFlagSync {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
struct ObjectFlagsInner {
    /// Indicates which "bank" of flags should be active for setting
    toggle: bool,
    /// A global flag that should be set by any object which has set a flag
    global_flag: bool,
}

impl ObjectFlagSync {
    /// Create a new ObjectFlagSync
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(UnsafeCell::new(ObjectFlagsInner {
                toggle: false,
                global_flag: false,
            })),
        }
    }

    /// Toggle the flag and return the global flag
    pub fn toggle(&self) -> bool {
        critical_section::with(|cs| {
            let inner = self.inner.borrow(cs).get();
            // Safety: This is the only place inner is accessed, and it is in a critical section
            unsafe {
                let global = (*inner).global_flag;
                (*inner).global_flag = false;
                (*inner).toggle = !(*inner).toggle;
                global
            }
        })
    }

    /// Get the current value of the flag
    ///
    /// `setting` should be true to set the global flag
    pub fn get_flag(&self, setting: bool) -> bool {
        critical_section::with(|cs| {
            let inner = unsafe { &mut (*self.inner.borrow(cs).get()) };
            inner.global_flag |= setting;
            inner.toggle
        })
    }
}

/// Stores an event flag for each sub object in an object
///
/// PDO transmission can be triggered by events, but PDOs are runtime configurable. An application
/// needs to be able to signal that an object has changed, and if that object is mapped to a TPDO,
/// that PDO should be scheduled for transmission.
///
/// In order to achieve this in a synchronized way without long critical sections, each object
/// holds two sets of flags, and they are swapped atomically using a global `ObjectFlagSync` shared by
/// all `ObjectFlags` instances.
#[allow(missing_debug_implementations)]
pub struct ObjectFlags<const N: usize> {
    sync: &'static ObjectFlagSync,
    flags0: AtomicCell<[u8; N]>,
    flags1: AtomicCell<[u8; N]>,
}

/// Trait for accessing object flags
pub trait ObjectFlagAccess {
    /// Set the flag for the specified sub object
    ///
    /// The flag is set on the currently active flag set
    fn set_flag(&self, sub: u8);
    /// Read the flag for the specified object
    ///
    /// The flag is read from the currently inactive flag set, i.e. the flag value from before the
    /// last sync toggle is returned
    fn get_flag(&self, sub: u8) -> bool;
    /// Clear all flags in the currently active flag set
    fn clear(&self);
}

impl<const N: usize> ObjectFlags<N> {
    /// Create a new ObjectFlags
    pub const fn new(sync: &'static ObjectFlagSync) -> Self {
        Self {
            sync,
            flags0: AtomicCell::new([0; N]),
            flags1: AtomicCell::new([0; N]),
        }
    }
}

impl<const N: usize> ObjectFlagAccess for ObjectFlags<N> {
    fn set_flag(&self, sub: u8) {
        if sub as usize >= N * 8 {
            return;
        }
        let flags = if self.sync.get_flag(true) {
            &self.flags0
        } else {
            &self.flags1
        };
        flags
            .fetch_update(|mut f| {
                f[sub as usize / 8] |= 1 << (sub & 7);
                Some(f)
            })
            .unwrap();
    }

    fn get_flag(&self, sub: u8) -> bool {
        if sub as usize >= N * 8 {
            return false;
        }
        let flags = if self.sync.get_flag(false) {
            &self.flags1.load()
        } else {
            &self.flags0.load()
        };
        flags[(sub / 8) as usize] & (1 << (sub & 7)) != 0
    }

    fn clear(&self) {
        if self.sync.get_flag(false) {
            self.flags1.store([0; N]);
        } else {
            self.flags0.store([0; N]);
        }
    }
}
