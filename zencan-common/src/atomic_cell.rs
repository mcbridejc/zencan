//! Implements an AtomicCell type which uses critical_section Mutex to enforce atomic store/load
//!
//! Since Crossbeam does not support thumbv6m due to lack of CAS, this is a fallback option.
//!
//! TODO: This can be made more efficient by using core::sync::atomic types for types which can
//! transmute into the base integer types.

use core::{cell::Cell, ops::Add};
use critical_section::Mutex;

use crate::traits::LoadStore;

/// A container to allow atomic access to the contained object
pub struct AtomicCell<T> {
    inner: Mutex<Cell<T>>,
}

impl<T: Send + Copy> AtomicCell<T> {
    /// Read the value of the AtomicCell
    pub fn load(&self) -> T {
        critical_section::with(|cs| self.inner.borrow(cs).get())
    }

    /// Borrow a reference to the contained value
    ///
    /// A critical section must be obtained by the called and provided
    pub fn borrow<'a>(&'a self, cs: critical_section::CriticalSection<'a>) -> &'a Cell<T> {
        self.inner.borrow(cs)
    }

    /// Perform atomic modification of the contained value
    ///
    /// This operation will be performed in a critical section, so it will block all IRQs until the
    /// function returns.
    pub fn fetch_update(&self, mut f: impl FnMut(T) -> Option<T>) -> Result<T, T> {
        critical_section::with(|cs| {
            let old_value = self.inner.borrow(cs).get();
            if let Some(new_value) = f(old_value) {
                self.inner.borrow(cs).set(new_value);
                Ok(old_value)
            } else {
                Err(old_value)
            }
        })
    }
}

impl<T: Send> AtomicCell<T> {
    /// Create a new AtomicCell with the provided value
    pub const fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(Cell::new(value)),
        }
    }

    /// Replace the value of the AtomicCell
    pub fn store(&self, value: T) {
        critical_section::with(|cs| self.inner.borrow(cs).set(value));
    }
}

impl<T: Send + Default> AtomicCell<T> {
    /// Return the contained value, and replace it with a default value
    pub fn take(&self) -> T {
        critical_section::with(|cs| self.inner.borrow(cs).take())
    }
}

impl<T: Copy + Add<Output = T>> AtomicCell<T> {
    /// Atomically add value to the contained value
    pub fn fetch_add(&self, value: T) -> T {
        critical_section::with(|cs| {
            let old_value = self.inner.borrow(cs).get();
            self.inner.borrow(cs).set(old_value + value);
            old_value
        })
    }
}

impl<T: Default> Default for AtomicCell<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Cell::new(T::default())),
        }
    }
}

impl<T: Default + Copy + Send> LoadStore<T> for AtomicCell<T> {
    fn load(&self) -> T {
        self.load()
    }

    fn store(&self, value: T) {
        self.store(value)
    }
}
