//! Traits and types for implementing objects in the OD

use zencan_common::{
    i24,
    object_model::{AccessType, DataType, ObjectCode},
    protocol::AbortCode,
    u24, AtomicCell,
};

use crate::ResetScope;

use super::{ObjectFlagAccess, SubInfo, SubObjectAccess};

/// A trait for accessing objects
///
/// Any struct which implements an object in the object dictionary must implement this trait
pub trait ObjectAccess: Sync + Send {
    /// Read raw bytes from a subobject
    ///
    /// If the specified read goes out of range (i.e. offset + buf.len() > current_size) an error is
    /// returned. All implementers are required to allow reading a subset of the object bytes, i.e.
    /// offset may be non-zero, and/or the buf length may be shorter than the object data
    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode>;

    /// Get the number of bytes available for a read
    fn read_size(&self, sub: u8) -> Result<usize, AbortCode>;

    /// Write raw bytes to a subobject
    ///
    /// The length of `data` must match the size of the object, or else it will fail with either
    /// [`AbortCode::DataTypeMismatchLengthLow`] or [`AbortCode::DataTypeMismatchLengthHigh`].
    ///
    /// If the sub is does not exist, it shall fail with [`AbortCode::NoSuchSubIndex`].
    ///
    /// If the sub exists but is not writeable, it shall fail with [`AbortCode::ReadOnly`].
    fn write(&self, sub: u8, data: &[u8]) -> Result<(), AbortCode>;

    /// Initialize a new partial write
    ///
    /// This must be called before performing calls to `partial_write`.
    ///
    /// A default implementation is provided which returns an appropriate error:
    /// - [`AbortCode::NoSuchSubIndex`] if the sub object does not exist
    /// - [`AbortCode::ReadOnly`] if the sub object is read-only
    /// - [`AbortCode::UnsupportedAccess`] if the sub object does not support partial writes
    ///
    /// Objects which support partial writing must override the default implementation.
    fn begin_partial(&self, sub: u8) -> Result<(), AbortCode> {
        if let Ok(sub_info) = self.sub_info(sub) {
            if sub_info.access_type.is_writable() {
                Err(AbortCode::UnsupportedAccess)
            } else {
                Err(AbortCode::ReadOnly)
            }
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    /// Perform a partial write of bytes to a subobject
    ///
    /// Most objects do not support partial writes. But in some cases, such as DOMAINs, or very
    /// large string objects, it is unavoidable and these must support it.
    ///
    /// Partial writes MUST be done sequentially, and implementers may assume that this is the case.
    /// Executing multiple concurrent partial writes to the same sub object is not supported. It is
    /// up to the application to ensure that this is not done.
    fn write_partial(&self, _sub: u8, _buf: &[u8]) -> Result<(), AbortCode> {
        // All callers should have failed at begin_partial, so this should never be called
        Err(AbortCode::GeneralError)
    }

    /// Finalize a previous partial write
    ///
    /// This must always be called after using partial_write, after all partial_write calls have
    /// been completed.
    fn end_partial(&self, _sub: u8) -> Result<(), AbortCode> {
        Err(AbortCode::GeneralError)
    }

    /// Get the type of this object
    fn object_code(&self) -> ObjectCode;

    /// Get metadata about a sub object
    fn sub_info(&self, sub: u8) -> Result<SubInfo, AbortCode>;

    /// Get the highest sub index available in this object
    fn max_sub_number(&self) -> u8 {
        match self.object_code() {
            ObjectCode::Null => 0,
            ObjectCode::Domain => 0,
            ObjectCode::DefType => 0,
            ObjectCode::DefStruct => 0,
            ObjectCode::Var => 0,
            ObjectCode::Array => self.read_u8(0).unwrap(),
            ObjectCode::Record => self.read_u8(0).unwrap(),
        }
    }

    /// Set an event flag for the specified sub object on this object
    ///
    /// Event flags are used for triggering PDOs. This is optional, as not all objects support PDOs
    /// or PDO triggering.
    fn set_event_flag(&self, _sub: u8) -> Result<(), AbortCode> {
        Err(AbortCode::UnsupportedAccess)
    }

    /// Read an event flag for the specified sub object
    ///
    /// This is optional as not all objects support events
    fn read_event_flag(&self, _sub: u8) -> bool {
        false
    }

    /// Clear event flags for all sub objects
    ///
    /// This is optional as not all objects support events
    fn clear_events(&self) {}

    /// Get the access type of a specific sub object
    fn access_type(&self, sub: u8) -> Result<AccessType, AbortCode> {
        Ok(self.sub_info(sub)?.access_type)
    }

    /// Get the data type of a specific sub object
    fn data_type(&self, sub: u8) -> Result<DataType, AbortCode> {
        Ok(self.sub_info(sub)?.data_type)
    }

    /// Get the maximum size of an sub object
    ///
    /// For most sub objects, this matches the current_size, but for strings the size of the
    /// currently stored value (returned by `current_size()`) may be smaller.
    fn size(&self, sub: u8) -> Result<usize, AbortCode> {
        Ok(self.sub_info(sub)?.size())
    }

    /// Get the current size of a sub object
    ///
    /// Note that this is not necessarily the allocated size of the object, as some objects (such as
    /// strings) may have values shorter than their maximum size. As such, this gives the maximum
    /// number of bytes which may be read, but not necessarily the number of bytes which may be
    /// written.
    fn current_size(&self, sub: u8) -> Result<usize, AbortCode> {
        const CHUNK_SIZE: usize = 8;

        let size = self.size(sub)?;
        if self.data_type(sub)?.is_str() {
            // Look for first 0
            let mut chunk = 0;
            let mut buf = [0; CHUNK_SIZE];
            while chunk < size / CHUNK_SIZE + 1 {
                let offset = chunk * CHUNK_SIZE;
                let bytes_to_read = (size - offset).min(CHUNK_SIZE);
                self.read(sub, offset, &mut buf[0..bytes_to_read])?;

                if let Some(zero_pos) = buf[0..bytes_to_read].iter().position(|b| *b == 0) {
                    return Ok(zero_pos + chunk * CHUNK_SIZE);
                }
                chunk += 1;
            }
        }
        // not a string type or no null-terminator was found
        Ok(size)
    }

    /// Read a sub object as a u32
    fn read_u32(&self, sub: u8) -> Result<u32, AbortCode> {
        let mut buf = [0; 4];
        self.read(sub, 0, &mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Read a sub object as a u24
    fn read_u24(&self, sub: u8) -> Result<u24, AbortCode> {
        let mut buf = [0; 3];
        self.read(sub, 0, &mut buf)?;
        Ok(u24::from_le_bytes(buf))
    }

    /// Read a sub object as a u16
    fn read_u16(&self, sub: u8) -> Result<u16, AbortCode> {
        let mut buf = [0; 2];
        self.read(sub, 0, &mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Read a sub object as a u8
    fn read_u8(&self, sub: u8) -> Result<u8, AbortCode> {
        let mut buf = [0; 1];
        self.read(sub, 0, &mut buf)?;
        Ok(buf[0])
    }

    /// Read a sub object as an i32
    fn read_i32(&self, sub: u8) -> Result<i32, AbortCode> {
        let mut buf = [0; 4];
        self.read(sub, 0, &mut buf)?;
        Ok(i32::from_le_bytes(buf))
    }

    /// Read a sub object as an i24
    fn read_i24(&self, sub: u8) -> Result<i24, AbortCode> {
        let mut buf = [0; 3];
        self.read(sub, 0, &mut buf)?;
        Ok(i24::from_le_bytes(buf))
    }

    /// Read a sub object as an i16
    fn read_i16(&self, sub: u8) -> Result<i16, AbortCode> {
        let mut buf = [0; 2];
        self.read(sub, 0, &mut buf)?;
        Ok(i16::from_le_bytes(buf))
    }

    /// Read a sub object as an i8
    fn read_i8(&self, sub: u8) -> Result<i8, AbortCode> {
        let mut buf = [0; 1];
        self.read(sub, 0, &mut buf)?;
        Ok(buf[0] as i8)
    }
}
/// A trait for structs which represent Objects to implement
///
/// Implementing this type allows a type sub object which implements [`SubObjectAccess`] to
/// implement [`ObjectAccess`] simply by implementing this trait to provide a sub object for each
/// sub index.
pub trait ProvidesSubObjects {
    /// Get a sub object by index
    ///
    /// This is used for objects which have a fixed number of sub objects, such as arrays or records.
    ///
    /// It should return None if the sub object does not exist, and when it does exist it returns a
    /// tuple containing a [`SubInfo`] with metadata about the sub object, and [`&dyn
    /// SubObjectAccess`](SubObjectAccess) which provides read/write access to the sub object data.
    fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)>;

    /// Get the object flags for this object
    ///
    /// Notification flags are supported by some objects to indicate changes made in the object by
    /// the application -- for example, to trigger the transmission of a mapped PDO.
    ///
    /// If the object supports flags, it should override this method to return a reference to them
    fn flags(&self) -> Option<&dyn ObjectFlagAccess> {
        None
    }

    /// What type of object is this
    fn object_code(&self) -> ObjectCode;
}

// Implement ObjectAccess for any type that implements ProvidesSubObjects
impl<T: ProvidesSubObjects + Sync + Send> ObjectAccess for T {
    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        if let Some((info, access)) = self.get_sub_object(sub) {
            if info.access_type.is_readable() {
                access.read(offset, buf)
            } else {
                Err(AbortCode::WriteOnly)
            }
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn read_size(&self, sub: u8) -> Result<usize, AbortCode> {
        if let Some((_info, access)) = self.get_sub_object(sub) {
            Ok(access.read_size())
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn write(&self, sub: u8, data: &[u8]) -> Result<(), AbortCode> {
        if let Some((info, access)) = self.get_sub_object(sub) {
            if info.access_type.is_writable() {
                access.write(data)
            } else {
                Err(AbortCode::ReadOnly)
            }
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn begin_partial(&self, sub: u8) -> Result<(), AbortCode> {
        if let Some((info, access)) = self.get_sub_object(sub) {
            if info.access_type.is_writable() {
                access.begin_partial()
            } else {
                Err(AbortCode::ReadOnly)
            }
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn write_partial(&self, sub: u8, buf: &[u8]) -> Result<(), AbortCode> {
        if let Some((_, access)) = self.get_sub_object(sub) {
            access.write_partial(buf)
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn end_partial(&self, sub: u8) -> Result<(), AbortCode> {
        if let Some((_, access)) = self.get_sub_object(sub) {
            access.end_partial()
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }

    fn set_event_flag(&self, sub: u8) -> Result<(), AbortCode> {
        if let Some(flags) = self.flags() {
            flags.set_flag(sub);
            Ok(())
        } else {
            Err(AbortCode::UnsupportedAccess)
        }
    }

    fn read_event_flag(&self, sub: u8) -> bool {
        if let Some(flags) = self.flags() {
            flags.get_flag(sub)
        } else {
            false
        }
    }

    fn clear_events(&self) {
        if let Some(flags) = self.flags() {
            flags.clear();
        }
    }

    fn object_code(&self) -> ObjectCode {
        self.object_code()
    }

    fn sub_info(&self, sub: u8) -> Result<SubInfo, AbortCode> {
        if let Some((info, _access)) = self.get_sub_object(sub) {
            Ok(info)
        } else {
            Err(AbortCode::NoSuchSubIndex)
        }
    }
}

/// OD placeholder for an object which will have a handler registered at runtime
#[allow(missing_debug_implementations)]
pub struct CallbackObject<'a> {
    obj: AtomicCell<Option<&'a dyn ObjectAccess>>,
    object_code: ObjectCode,
}

impl<'a> CallbackObject<'a> {
    /// Create a new callback
    pub const fn new(object_code: ObjectCode) -> Self {
        Self {
            obj: AtomicCell::new(None),
            object_code,
        }
    }

    /// Register callback handler for object accesses
    pub fn register_handler(&self, handler: &'a dyn ObjectAccess) {
        self.obj.store(Some(handler))
    }
}

impl ObjectAccess for CallbackObject<'_> {
    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        if let Some(obj) = self.obj.load() {
            obj.read(sub, offset, buf)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn read_size(&self, sub: u8) -> Result<usize, AbortCode> {
        if let Some(obj) = self.obj.load() {
            obj.read_size(sub)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn write(&self, sub: u8, data: &[u8]) -> Result<(), AbortCode> {
        if let Some(obj) = self.obj.load() {
            obj.write(sub, data)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn write_partial(&self, sub: u8, buf: &[u8]) -> Result<(), AbortCode> {
        if let Some(obj) = self.obj.load() {
            obj.write_partial(sub, buf)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn end_partial(&self, sub: u8) -> Result<(), AbortCode> {
        if let Some(obj) = self.obj.load() {
            obj.end_partial(sub)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn object_code(&self) -> ObjectCode {
        self.object_code
    }

    fn sub_info(&self, sub: u8) -> Result<SubInfo, AbortCode> {
        if let Some(obj) = self.obj.load() {
            obj.sub_info(sub)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }
}

/// Represents one item in a table of objects.
///
/// Entries contain references only to fully initialized objects.
#[allow(missing_debug_implementations)]
#[repr(C)]
pub struct ODEntry<'a> {
    /// The object index
    pub index: u16,
    /// The object implementation
    pub object: &'a dyn ObjectAccess,
}

/// Read-only table entry whose object reference becomes valid after initialization.
///
/// Stores raw trait-object pointers without forming references to uninitialized objects. This extra
/// unsafe code allows the OD table to remain in .rodata / flash rather than using RAM. The objects
/// are all static, so we know the addresses, but they are not initialized by the runtime, so
/// creating a ref before init is not allowed.
#[doc(hidden)]
#[allow(missing_debug_implementations)]
#[repr(C)]
pub struct ODEntryUninit {
    index: u16,
    object: *const dyn ObjectAccess,
}

// SAFETY: Generated entries contain immutable pointers to static ObjectAccess
// allocations. The pointers themselves are never mutated after publication.
unsafe impl Sync for ODEntryUninit {}

impl ODEntryUninit {
    /// Record an object's address and vtable without accessing its storage.
    pub const fn from_ptr(index: u16, object: *const dyn ObjectAccess) -> Self {
        Self { index, object }
    }

    /// View the read-only table after all referenced objects are published.
    ///
    /// # Safety
    /// Every stored pointer must refer to a fully initialized, static ObjectAccess
    /// implementation and remain valid for the rest of the program. Initialization
    /// must happen-before this call; entries must never subsequently be mutated.
    #[inline(always)]
    pub unsafe fn assume_init_slice(entries: &'static [Self]) -> &'static [ODEntry<'static>] {
        // SAFETY: repr(C), corresponding field types, and the checks below ensure
        // identical layout. The caller guarantees all reference bytes are valid.
        unsafe { core::slice::from_raw_parts(entries.as_ptr().cast(), entries.len()) }
    }
}

const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    assert!(size_of::<ODEntryUninit>() == size_of::<ODEntry<'static>>());
    assert!(align_of::<ODEntryUninit>() == align_of::<ODEntry<'static>>());
    assert!(offset_of!(ODEntryUninit, index) == offset_of!(ODEntry<'static>, index));
    assert!(offset_of!(ODEntryUninit, object) == offset_of!(ODEntry<'static>, object));
};

/// Provides object lookup by index.
pub trait ObjectLookup<'a>: Sync {
    /// Lookup an object by index
    fn find_object(&self, index: u16) -> Option<&'a dyn ObjectAccess>;
}

/// A trait which provides the contract for the ObjectDict struct generated by zencan-build,
/// which must be accessed via dynamic dispatch because it is generated in the application.
pub trait ObjectDictionary<'a>: ObjectLookup<'a> {
    /// Reset objects to their default value
    ///
    /// `scope` specifies whether all objects or only communications objects should be reset
    fn reset_objects(&self, scope: ResetScope);

    /// Returns a slice of ODEntry with all objects in the dict
    ///
    /// The slice is always ordered by object index
    fn od_table(&self) -> &'a [ODEntry<'a>];
}

impl<'a> ObjectLookup<'a> for &[ODEntry<'a>] {
    fn find_object(&self, index: u16) -> Option<&'a dyn ObjectAccess> {
        find_object(self, index)
    }
}

impl<'a, const N: usize> ObjectLookup<'a> for [ODEntry<'a>; N] {
    fn find_object(&self, index: u16) -> Option<&'a dyn ObjectAccess> {
        self.as_slice().find_object(index)
    }
}

/// Lookup an object from the Object dictionary table
///
/// Note: `table` must be sorted by index
pub fn find_object<'a>(table: &[ODEntry<'a>], index: u16) -> Option<&'a dyn ObjectAccess> {
    table
        .binary_search_by_key(&index, |e| e.index)
        .ok()
        .map(|i| table[i].object)
}

/// Helper for extracting read data from object bytes
pub(crate) fn read_object<const N: usize>(
    value: [u8; N],
    offset: usize,
    buf: &mut [u8],
) -> Result<usize, AbortCode> {
    if offset < value.len() {
        let read_len = buf.len().min(value.len() - offset);
        buf[0..read_len].copy_from_slice(&value[offset..offset + read_len]);
        Ok(read_len)
    } else {
        Ok(0)
    }
}

/// Helper for getting an expected number of bytes from write data during object write
pub(crate) fn get_object_write_data<const N: usize>(data: &[u8]) -> Result<[u8; N], AbortCode> {
    if data.len() < N {
        return Err(AbortCode::DataTypeMismatchLengthLow);
    } else if data.len() > N {
        return Err(AbortCode::DataTypeMismatchLengthHigh);
    }
    let mut array = [0; N];
    array.copy_from_slice(data);
    Ok(array)
}
