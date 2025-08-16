//! Bootloader objects
//!
//!

use core::sync::atomic::{AtomicBool, Ordering};

use crate::object_dict::{
    ConstByteRefField, ConstField, ObjectAccess, ProvidesSubObjects, SubObjectAccess,
};
use zencan_common::{
    constants::values::BOOTLOADER_ERASE_CMD,
    objects::{ObjectCode, SubInfo},
    sdo::AbortCode,
    AtomicCell,
};

/// Implements a Bootloader info (0x5500) object
#[derive(Debug, Default)]
pub struct BootloaderInfo<const APP: bool, const NUM_SECTIONS: u8> {
    reset_flag: ResetField,
}

impl<const APP: bool, const NUM_SECTIONS: u8> BootloaderInfo<APP, NUM_SECTIONS> {
    /// Create new BootloaderInfo
    pub const fn new() -> Self {
        Self {
            reset_flag: ResetField::new(),
        }
    }

    /// Read the reset_flag
    ///
    /// The flag is set when a reset command is written to the object, and this function can be used
    /// by the application to determed when a reset to bootloader is commanded
    pub fn reset_flag(&self) -> bool {
        self.reset_flag.load()
    }
}

#[derive(Debug, Default)]
struct ResetField {
    flag: AtomicBool,
}

impl ResetField {
    pub const fn new() -> Self {
        Self {
            flag: AtomicBool::new(false),
        }
    }

    pub fn load(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

impl SubObjectAccess for ResetField {
    fn read(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize, AbortCode> {
        Err(AbortCode::WriteOnly)
    }

    fn read_size(&self) -> usize {
        0
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        if data.len() == 4 {
            if data == [0x42, 0x4F, 0x4F, 0x54] {
                self.flag.store(true, Ordering::Relaxed);
                Ok(())
            } else {
                Err(AbortCode::InvalidValue)
            }
        } else if data.len() < 4 {
            Err(AbortCode::DataTypeMismatchLengthLow)
        } else {
            Err(AbortCode::DataTypeMismatchLengthHigh)
        }
    }
}

/// Get the value to return for the config object
///
/// `app` - Indicates whether the current node is running as an application, rather than a
/// bootloader
const fn get_config_value(app: bool) -> u32 {
    let mut config = 1;
    if app {
        config |= 2;
    }
    config
}

impl<const APP: bool, const NUM_SECTIONS: u8> ProvidesSubObjects
    for BootloaderInfo<APP, NUM_SECTIONS>
{
    fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
        match sub {
            0 => Some((
                SubInfo::MAX_SUB_NUMBER,
                const { &ConstField::new(3u8.to_le_bytes()) },
            )),
            1 => Some((SubInfo::new_u32().ro_access(), {
                const { &ConstField::new(get_config_value(APP).to_le_bytes()) }
            })),
            2 => Some((SubInfo::new_u8().ro_access(), {
                const { &ConstField::new(NUM_SECTIONS.to_le_bytes()) }
            })),
            3 => Some((SubInfo::new_u32().wo_access(), &self.reset_flag)),
            _ => None,
        }
    }

    fn object_code(&self) -> ObjectCode {
        ObjectCode::Record
    }
}

/// A trait for applications to implement to provide a bootloader section access implementation
pub trait BootloaderSectionCallbacks: Sync {
    /// Called to erase the section
    ///
    /// Returns true if section is successfully erased and ready for programming
    fn erase(&self) -> bool;

    /// Write a chunk of data
    ///
    /// Write will be called 1 or more times after an erase with a sequence of new data to write to
    /// the section
    fn write(&self, data: &[u8]);

    /// Finalize writing a section
    ///
    /// Will be called once after all data has been written to allow the storage driver to finalize
    /// writing the data and return any errors.
    ///
    /// Returns true on successful write
    fn finalize(&self) -> bool;
}

/// Implements a bootloader section object in the object dictionary
#[allow(missing_debug_implementations)]
pub struct BootloaderSection {
    name: &'static str,
    size: u32,
    callbacks: AtomicCell<Option<&'static dyn BootloaderSectionCallbacks>>,
}

impl BootloaderSection {
    /// Create a new bootloader section
    pub const fn new(name: &'static str, size: u32) -> Self {
        Self {
            name,
            size,
            callbacks: AtomicCell::new(None),
        }
    }

    /// Register the application callbacks which implement storage for this section
    pub fn register_callbacks(&self, callbacks: &'static dyn BootloaderSectionCallbacks) {
        self.callbacks.store(Some(callbacks));
    }
}

impl ObjectAccess for BootloaderSection {
    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        match sub {
            0 => ConstField::new(4u8.to_le_bytes()).read(offset, buf),
            1 => ConstField::new(1u8.to_le_bytes()).read(offset, buf),
            2 => ConstByteRefField::new(self.name.as_bytes()).read(offset, buf),
            3 => Err(AbortCode::WriteOnly),
            4 => Err(AbortCode::WriteOnly),
            _ => Err(AbortCode::NoSuchSubIndex),
        }
    }

    fn read_size(&self, sub: u8) -> Result<usize, AbortCode> {
        match sub {
            0 => Ok(1),
            1 => Ok(1),
            2 => Ok(self.name.len()),
            3 => Ok(0),
            4 => Ok(0),
            _ => Err(AbortCode::NoSuchSubIndex),
        }
    }

    fn write(&self, sub: u8, data: &[u8]) -> Result<(), AbortCode> {
        match sub {
            0 => Err(AbortCode::ReadOnly),
            1 => Err(AbortCode::ReadOnly),
            2 => Err(AbortCode::ReadOnly),
            3 => {
                if data == BOOTLOADER_ERASE_CMD.to_le_bytes() {
                    if let Some(cb) = self.callbacks.load() {
                        if cb.erase() {
                            Ok(())
                        } else {
                            Err(AbortCode::GeneralError)
                        }
                    } else {
                        Err(AbortCode::ResourceNotAvailable)
                    }
                } else {
                    Err(AbortCode::InvalidValue)
                }
            }
            4 => {
                if let Some(callbacks) = self.callbacks.load() {
                    callbacks.write(data);
                    if callbacks.finalize() {
                        // success
                        Ok(())
                    } else {
                        Err(AbortCode::GeneralError)
                    }
                } else {
                    Err(AbortCode::ResourceNotAvailable)
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
            1 => Ok(SubInfo::new_u8().ro_access()),
            2 => Ok(SubInfo::new_visibile_str(self.name.len()).ro_access()),
            3 => Ok(SubInfo::new_u32().wo_access()),
            4 => Ok(SubInfo {
                size: self.size as usize,
                data_type: zencan_common::objects::DataType::Domain,
                access_type: zencan_common::objects::AccessType::Rw,
                pdo_mapping: zencan_common::objects::PdoMapping::None,
                persist: false,
            }),
            _ => Err(AbortCode::NoSuchSubIndex),
        }
    }
}
