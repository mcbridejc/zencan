//! One-time storage used by generated object dictionaries.
use core::{cell::UnsafeCell, mem::MaybeUninit};
use portable_atomic::{AtomicU8, Ordering};

/// A guarded allocation which publishes a value only after construction.
///
/// Initialization runs in a critical section. A tri-state tracker is used to prevent recursive
/// init, or a panic during init.
#[allow(missing_debug_implementations)]
pub struct InitCell<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Send + Sync> Sync for InitCell<T> {}

impl<T> InitCell<T> {
    /// Allocate uninitialized storage
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(0),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Return the value if construction has completed, otherwise return None.
    #[inline(always)]
    pub fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == 2 {
            // SAFETY: Publication follows construction and the value is never replaced.
            Some(unsafe { (*self.value.get()).assume_init_ref() })
        } else {
            None
        }
    }

    /// Get the initialized value, or initialize if needed
    #[inline]
    pub fn get_or_init(&self, init: impl FnOnce() -> T) -> &T {
        if let Some(value) = self.get() {
            return value;
        }
        // SAFETY: The closure writes a complete T before publication.
        unsafe { self.initialize(|ptr| ptr.write(init())) }
    }

    /// Construct a static value in place under one initialization guard.
    ///
    /// # Safety
    /// The initializer must fully initialize T before returning. It may form
    /// references to already initialized fields, but must not expose the value
    /// outside construction or move fields after taking references to them.
    /// The pointer denotes aligned, exclusive, uninitialized static storage.
    /// A panic poisons the cell permanently; partial construction is not retried.
    #[inline]
    pub unsafe fn get_or_init_in_place(&'static self, init: impl FnOnce(*mut T)) -> &'static T {
        if let Some(value) = self.get() {
            return value;
        }
        // SAFETY: Forward the caller's initialization contract.
        unsafe { self.initialize(init) }
    }

    #[cold]
    #[inline(never)]
    unsafe fn initialize(&self, init: impl FnOnce(*mut T)) -> &T {
        critical_section::with(|_| {
            match self.state.load(Ordering::Relaxed) {
                2 => return,
                0 => self.state.store(1, Ordering::Relaxed),
                _ => panic!("recursive or poisoned dictionary initialization"),
            }
            init(self.value.get().cast::<T>());
            self.state.store(2, Ordering::Release);
        });
        // SAFETY: The initializer establishes a complete T before publication.
        unsafe { (*self.value.get()).assume_init_ref() }
    }
}

impl<T> Default for InitCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Static object storage without an independent initialization guard.
///
/// Generated dictionaries initialize these allocations under their shared guard.
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct StaticStorage<T> {
    value: UnsafeCell<MaybeUninit<T>>,
}

// SAFETY: Mutation and publication require the unsafe methods' external guard
// contract. Once published, sharing the initialized value requires Send + Sync.
unsafe impl<T: Send + Sync> Sync for StaticStorage<T> {}

impl<T> StaticStorage<T> {
    /// Allocate uninitialized static storage.
    pub const fn new() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Address of the eventual object, usable in static raw-pointer tables.
    pub const fn as_ptr(&self) -> *mut T {
        self.value.get().cast::<T>()
    }

    /// Construct the object at its final address.
    ///
    /// # Safety
    /// The caller must hold the shared initialization guard and write this
    /// allocation exactly once, before exposing any references to its contents.
    #[inline]
    pub unsafe fn write(&'static self, value: T) -> &'static T {
        unsafe {
            self.as_ptr().write(value);
            self.assume_init_ref()
        }
    }

    /// Access a published object without checking a guard.
    ///
    /// # Safety
    /// The object must have been fully initialized and published through the
    /// dictionary guard, and must never be moved or overwritten afterward.
    #[inline(always)]
    pub unsafe fn assume_init_ref(&'static self) -> &'static T {
        unsafe { &*self.as_ptr() }
    }
}

impl<T> Default for StaticStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    #[test]
    fn initialization_is_once_and_panic_poison_is_sticky() {
        let cell = InitCell::new();
        assert_eq!(*cell.get_or_init(|| 17), 17);
        assert_eq!(*cell.get_or_init(|| panic!("must not run")), 17);
        let poisoned = InitCell::<u32>::new();
        assert!(catch_unwind(AssertUnwindSafe(
            || poisoned.get_or_init(|| panic!("failed"))
        ))
        .is_err());
        assert!(catch_unwind(AssertUnwindSafe(|| poisoned.get_or_init(|| 42))).is_err());
    }

    #[test]
    fn in_place_initialization_preserves_internal_references() {
        struct Linked {
            value: core::sync::atomic::AtomicU32,
            link: &'static core::sync::atomic::AtomicU32,
        }
        static CELL: InitCell<Linked> = InitCell::new();
        // SAFETY: Both fields are written at their final addresses. The link
        // points to a fully initialized field and neither is subsequently moved.
        let value = unsafe {
            CELL.get_or_init_in_place(|ptr| {
                assert!(CELL.get().is_none());
                let field = core::ptr::addr_of_mut!((*ptr).value);
                field.write(core::sync::atomic::AtomicU32::new(42));
                core::ptr::addr_of_mut!((*ptr).link).write(&*field);
            })
        };
        assert!(core::ptr::eq(&value.value, value.link));
        value.value.store(99, Ordering::Relaxed);
        assert_eq!(CELL.get().unwrap().link.load(Ordering::Relaxed), 99);
        assert!(core::ptr::eq(
            value,
            CELL.get_or_init(|| panic!("must not rerun"))
        ));
    }

    #[test]
    fn partial_in_place_initialization_is_not_published_or_retried() {
        static CELL: InitCell<[u32; 2]> = InitCell::new();
        assert!(catch_unwind(AssertUnwindSafe(|| unsafe {
            CELL.get_or_init_in_place(|ptr| {
                ptr.cast::<u32>().write(42);
                panic!("partial initialization");
            })
        }))
        .is_err());
        assert!(CELL.get().is_none());
        assert!(catch_unwind(AssertUnwindSafe(|| CELL.get_or_init(|| [1, 2]))).is_err());
    }

    #[test]
    fn recursive_initialization_cannot_publish_a_partial_value() {
        let cell = InitCell::<u32>::new();
        assert!(catch_unwind(AssertUnwindSafe(
            || cell.get_or_init(|| *cell.get_or_init(|| 42))
        ))
        .is_err());
        assert!(catch_unwind(AssertUnwindSafe(|| cell.get_or_init(|| 7))).is_err());
    }
}
