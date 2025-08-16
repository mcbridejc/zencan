//! Handling for persistent storage control objects
//!
//!

use core::convert::Infallible;

use zencan_common::{
    constants::values::SAVE_CMD,
    objects::{ObjectCode, SubInfo},
    sdo::AbortCode,
    AtomicCell,
};

use crate::object_dict::{ODEntry, ObjectAccess};

/// A callback function type for handling a store objects event
pub type StoreObjectsCallback =
    dyn Fn(&mut dyn embedded_io::Read<Error = Infallible>, usize) + Sync;

#[derive(Default)]
#[allow(missing_debug_implementations)]
/// Shared state for supporting object storage
pub struct StorageContext {
    pub(crate) store_callback: AtomicCell<Option<&'static StoreObjectsCallback>>,
}

impl StorageContext {
    /// Create a new StorageContext
    pub const fn new() -> Self {
        Self {
            store_callback: AtomicCell::new(None),
        }
    }
}

/// Implements the storage command object (0x1010)
#[allow(missing_debug_implementations)]
pub struct StorageCommandObject {
    od: &'static [ODEntry<'static>],
    storage_context: &'static StorageContext,
}

impl StorageCommandObject {
    /// Create a new storage context object
    pub const fn new(
        od: &'static [ODEntry<'static>],
        storage_context: &'static StorageContext,
    ) -> Self {
        Self {
            od,
            storage_context,
        }
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
                if self.storage_context.store_callback.load().is_some() {
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
                        if let Some(cb) = self.storage_context.store_callback.load() {
                            crate::persist::serialize(self.od, cb);
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
