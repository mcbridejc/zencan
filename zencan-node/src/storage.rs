//! Handling for persistent storage control objects
//!
//!

use core::{convert::Infallible, sync::atomic::Ordering};

use portable_atomic::AtomicBool;
use zencan_common::{
    constants::values::SAVE_CMD,
    objects::{ObjectCode, SubInfo},
    sdo::AbortCode,
};

use crate::object_dict::ObjectAccess;

/// A callback function type for handling a store objects event
pub type StoreObjectsCallback =
    dyn Fn(&mut dyn embedded_io::Read<Error = Infallible>, usize) + Sync;

#[derive(Default)]
#[allow(missing_debug_implementations)]
/// Shared state for supporting object storage
pub struct StorageContext {
    /// A flag set by the storage command object when a store command is received
    pub(crate) store_flag: AtomicBool,
    /// Indicates to storage command object if storage is supported by the application
    pub(crate) store_supported: AtomicBool,
}

impl StorageContext {
    /// Create a new StorageContext
    pub const fn new() -> Self {
        Self {
            store_flag: AtomicBool::new(false),
            store_supported: AtomicBool::new(false),
        }
    }
}

/// Implements the storage command object (0x1010)
#[allow(missing_debug_implementations)]
pub struct StorageCommandObject {
    storage_context: &'static StorageContext,
}

impl StorageCommandObject {
    /// Create a new storage context object
    pub const fn new(storage_context: &'static StorageContext) -> Self {
        Self { storage_context }
    }
}

impl ObjectAccess for StorageCommandObject {
    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        match sub {
            0 => {
                if offset != 0 || buf.len() != 1 {
                    Err(AbortCode::DataTypeMismatch)
                } else {
                    buf[0] = 1;
                    Ok(1)
                }
            }
            1 => {
                // Bit 0 indicates the node is capable of saving objects. Set it if a callback has
                // been registered.
                let mut value = 0u32;
                if self.storage_context.store_supported.load(Ordering::Relaxed) {
                    value |= 1;
                }
                let value_bytes = value.to_le_bytes();
                if offset < value_bytes.len() {
                    let read_len = buf.len().min(value_bytes.len() - offset);
                    buf[..read_len].copy_from_slice(&value_bytes[offset..offset + read_len]);
                    Ok(read_len)
                } else {
                    Ok(0)
                }
            }
            _ => Err(AbortCode::NoSuchSubIndex),
        }
    }

    fn read_size(&self, sub: u8) -> Result<usize, AbortCode> {
        match sub {
            0 => Ok(1),
            1 => Ok(4),
            _ => Err(AbortCode::NoSuchSubIndex),
        }
    }

    fn write(&self, sub: u8, data: &[u8]) -> Result<(), AbortCode> {
        match sub {
            0 => Err(AbortCode::ReadOnly),
            1 => {
                if data.len() != 4 {
                    Err(AbortCode::DataTypeMismatch)
                } else {
                    let value = u32::from_le_bytes(data[0..4].try_into().unwrap());
                    // Magic value ('save') triggering a save
                    if value == SAVE_CMD {
                        if self.storage_context.store_supported.load(Ordering::Relaxed) {
                            self.storage_context
                                .store_flag
                                .store(true, Ordering::Relaxed);
                            Ok(())
                        } else {
                            Err(AbortCode::ResourceNotAvailable)
                        }
                    } else {
                        Err(AbortCode::IncompatibleParameter)
                    }
                }
            }
            _ => Err(AbortCode::NoSuchSubIndex),
        }
    }

    fn object_code(&self) -> ObjectCode {
        ObjectCode::Record
    }

    fn sub_info(&self, sub: u8) -> Result<SubInfo, AbortCode> {
        match sub {
            0 => Ok(SubInfo::MAX_SUB_NUMBER),
            1 => Ok(SubInfo::new_u32().rw_access()),
            _ => Err(AbortCode::NoSuchSubIndex),
        }
    }
}
