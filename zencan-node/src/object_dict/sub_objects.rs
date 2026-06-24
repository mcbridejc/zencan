//! Collection of generic fields which implement a sub-object

use core::{cell::UnsafeCell, sync::atomic::AtomicI16};

use super::objects::{get_object_write_data, read_object};
use portable_atomic::{
    AtomicBool, AtomicF32, AtomicF64, AtomicI32, AtomicI64, AtomicI8, AtomicU16, AtomicU32,
    AtomicU64, AtomicU8, Ordering,
};
use zencan_common::{
    i24,
    object_model::{ReadSize, TimeDifference, TimeOfDay},
    protocol::AbortCode,
    u24, AtomicCell,
};

/// Allow transparent byte level access to a sub object
pub trait SubObjectAccess: Sync + Send {
    /// Read data from the sub object
    ///
    /// Read up to `buf.len()` bytes, starting at offset
    ///
    /// # Arguments
    ///
    /// - `offset`: The offset into the object to begin the read
    /// - `buf`: The output buffer to fill
    ///
    /// # Returns
    ///
    /// The number of bytes read into `buf`, on success
    ///
    /// # Errors
    ///
    /// - [`AbortCode::WriteOnly`] if the sub object does not support reading
    /// - [`AbortCode::ResourceNotAvailable`] if the sub object depends on a runtime registered
    ///   handler which has not been registered
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode>;

    /// Return the amount of data which can be read
    ///
    /// This should return the maximum number of bytes which can be read from the object
    ///
    /// For most objects, this is simply it's allocated size -- e.g. for a UInt32 DataType
    /// `read_size` always return 4. Others may have variable or run-time determined size, for
    /// example VisibleString objects can ctonshorter than their allocated size.
    ///
    /// Note: Callers cannot assume that a call to read will return the same number of bytes as
    /// read_size. It is possible for the available number of bytes to change between a call to
    /// read_size and a subsequent call to read.
    fn read_size(&self) -> usize;

    /// Write data to the sub object
    ///
    /// For most objects, the length of data must match the size of the object exactly. However, for
    /// some objects, such as Domain, VisibleString, UnicodeString, or objects with custom
    /// callback implementations it may be possible to write shorter values.
    ///
    /// # Errors
    ///
    /// - [`AbortCode::DataTypeMismatchLengthHigh`] if `data.len()` exceeds the object size
    /// - [`AbortCode::DataTypeMismatchLengthLow`] if `data.len()` is smaller than the object size
    ///   and the object does not support this
    /// - [`AbortCode::ReadOnly`] if the object does not support writing
    /// - [`AbortCode::InvalidValue`] if the value is not allowed
    /// - [`AbortCode::ValueTooHigh`] if the value is higher than the allowed range of this object
    /// - [`AbortCode::ValueTooLow`] if the value is lower than the allowed range of this object
    /// - [`AbortCode::ResourceNotAvailable`] if the object cannot be written because of the
    ///   application state. For example, this is returned if a required callback has not been
    ///   registered on the object.
    ///
    /// Other error types may be returned by special purpose objects implemented via custom
    /// callback.
    fn write(&self, data: &[u8]) -> Result<(), AbortCode>;

    /// Begin a multi-part write to the object
    ///
    /// Not all objects support partial writes. Primarily it is large objects which support it in
    /// order to allow transfer of data in multiple blocks. It is up to the application to ensure
    /// that no other writes occur while a partial write is in progress, or else the object data may
    /// be corrupted and/or a call to `write_partial` may return an abort code on a subsequent call.
    ///
    /// Partial writes should always be performed according to the following sequence:
    /// - One call to `begin_partial`
    /// - Zero or more calls to `write_partial`
    /// - One call to `end_partial`
    ///
    /// # Errors
    ///
    /// - [`AbortCode::UnsupportedAccess] if the object does not support partial writes
    /// - [`AbortCode::ReadOnly`] if the object does not support writing
    /// - [`AbortCode::ResourceNotAvailable`] if the object cannot be written because of the
    ///   application state. For example, this is returned if a required callback has not been
    ///   registered on the object.
    fn begin_partial(&self) -> Result<(), AbortCode> {
        Err(AbortCode::UnsupportedAccess)
    }

    /// Write part of multi-part data to the object
    ///
    /// Calls to write_partial must be preceded by a call to begin_partial, and followed by a call
    /// to end_partial.
    ///
    /// # Errors
    /// - [`AbortCode::DataTypeMismatchLengthHigh`] if `data.len()` exceeds the object size
    fn write_partial(&self, _buf: &[u8]) -> Result<(), AbortCode> {
        Err(AbortCode::UnsupportedAccess)
    }

    /// Finish a multi-part write
    fn end_partial(&self) -> Result<(), AbortCode> {
        Err(AbortCode::UnsupportedAccess)
    }
}

/// A sub object which contains a single scalar value of type T, which is a standard rust type
#[allow(missing_debug_implementations)]
pub struct ScalarField<T: Copy> {
    value: AtomicCell<T>,
}

impl<T: Send + Copy + PartialEq> ScalarField<T> {
    /// Atomically read the value of the field
    pub fn load(&self) -> T {
        self.value.load()
    }

    /// Atomically store a new value into the field
    pub fn store(&self, value: T) {
        self.value.store(value);
    }
}

impl<T: Copy + Default> Default for ScalarField<T> {
    fn default() -> Self {
        Self {
            value: AtomicCell::default(),
        }
    }
}

macro_rules! impl_scalar_field {
    ($struct_name: ident, $atomic_type: ty, $rust_type: ty) => {
        /// A sub object which contains a single scalar value of type T, which is a standard rust type
        #[allow(missing_debug_implementations)]
        pub struct $struct_name {
            value: $atomic_type,
        }
        impl $struct_name {
            /// Create a new ScalarField with the given value
            pub const fn new(value: $rust_type) -> Self {
                Self {
                    value: <$atomic_type>::new(value),
                }
            }

            /// Atomically store a new value
            pub fn store(&self, value: $rust_type) {
                self.value.store(value, Ordering::Release)
            }

            /// Atomically load the value
            pub fn load(&self) -> $rust_type {
                self.value.load(Ordering::Acquire)
            }
        }
        impl Default for $struct_name {
            fn default() -> Self {
                Self {
                    value: Default::default(),
                }
            }
        }
        impl SubObjectAccess for $struct_name {
            fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
                let bytes = self.value.load(Ordering::Acquire).to_le_bytes();
                read_object(bytes, offset, buf)
            }

            fn read_size(&self) -> usize {
                <$rust_type as ReadSize>::READ_SIZE
            }

            fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
                let value = <$rust_type>::from_le_bytes(get_object_write_data(data)?);
                self.value.store(value, Ordering::Release);
                Ok(())
            }
        }
    };
}

impl_scalar_field!(ScalarFieldU8, AtomicU8, u8);
impl_scalar_field!(ScalarFieldU16, AtomicU16, u16);
impl_scalar_field!(ScalarFieldU32, AtomicU32, u32);
impl_scalar_field!(ScalarFieldU64, AtomicU64, u64);
impl_scalar_field!(ScalarFieldI8, AtomicI8, i8);
impl_scalar_field!(ScalarFieldI16, AtomicI16, i16);
impl_scalar_field!(ScalarFieldI32, AtomicI32, i32);
impl_scalar_field!(ScalarFieldI64, AtomicI64, i64);
impl_scalar_field!(ScalarFieldF32, AtomicF32, f32);
impl_scalar_field!(ScalarFieldF64, AtomicF64, f64);

/// Signed 24-bit sub object
#[derive(Debug)]
pub struct ScalarFieldI24 {
    value: AtomicU32,
}

impl ScalarFieldI24 {
    /// Create new ScalarFieldI24
    pub fn new(value: i24) -> Self {
        Self {
            value: AtomicU32::new(value.to_bits()),
        }
    }

    /// Atomically store a new value to the object
    pub fn store(&self, value: i24) {
        self.value.store(value.to_bits(), Ordering::Release)
    }

    /// Atomically read the object value
    pub fn load(&self) -> i24 {
        // Safety: We only store using i24::to_bits, so it must always be in range
        unsafe { i24::from_bits_unchecked(self.value.load(Ordering::Acquire)) }
    }
}

impl Default for ScalarFieldI24 {
    fn default() -> Self {
        Self {
            value: AtomicU32::new(i24::default().to_bits()),
        }
    }
}

impl SubObjectAccess for ScalarFieldI24 {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let bytes = self.load().to_le_bytes();
        read_object(bytes, offset, buf)
    }

    fn read_size(&self) -> usize {
        i24::READ_SIZE
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        let value = i24::from_le_bytes(get_object_write_data(data)?);
        self.store(value);
        Ok(())
    }
}

/// Unsigned 24-bit sub object
#[derive(Debug)]
pub struct ScalarFieldU24 {
    value: AtomicU32,
}

impl ScalarFieldU24 {
    /// Create new ScalarFieldU24
    pub fn new(value: u24) -> Self {
        Self {
            value: AtomicU32::new(value.value()),
        }
    }

    /// Atomically store a new value to the object
    pub fn store(&self, value: u24) {
        self.value.store(value.value(), Ordering::Release)
    }

    /// Atomically read the object value
    pub fn load(&self) -> u24 {
        // Safety: We only store using i24::to_bits, so it must always be in range
        unsafe { u24::new_unchecked(self.value.load(Ordering::Acquire)) }
    }
}

impl Default for ScalarFieldU24 {
    fn default() -> Self {
        Self {
            value: AtomicU32::new(u24::default().value()),
        }
    }
}

impl SubObjectAccess for ScalarFieldU24 {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let bytes = self.load().to_le_bytes();
        read_object(bytes, offset, buf)
    }

    fn read_size(&self) -> usize {
        u24::READ_SIZE
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        let value = u24::from_le_bytes(get_object_write_data(data)?);
        self.store(value);
        Ok(())
    }
}

/// A sub-object which stores a boolean
#[derive(Debug, Default)]
pub struct ScalarFieldBool {
    value: AtomicBool,
}

impl ScalarFieldBool {
    /// Create a new field
    pub const fn new(value: bool) -> Self {
        Self {
            value: AtomicBool::new(value),
        }
    }

    /// Store a new value
    pub fn store(&self, value: bool) {
        self.value.store(value, Ordering::Release)
    }

    /// Load the current value
    pub fn load(&self) -> bool {
        self.value.load(Ordering::Acquire)
    }
}

impl SubObjectAccess for ScalarFieldBool {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let value = self.value.load(Ordering::Acquire);
        if offset != 0 || buf.len() > 1 {
            return Err(AbortCode::DataTypeMismatchLengthHigh);
        }
        buf[0] = if value { 1 } else { 0 };
        Ok(1)
    }

    fn read_size(&self) -> usize {
        1
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        if data.len() != 1 {
            return Err(AbortCode::DataTypeMismatchLengthHigh);
        }
        let value = data[0] != 0;
        self.value.store(value, Ordering::Release);
        Ok(())
    }
}

impl SubObjectAccess for ScalarField<TimeDifference> {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let value = self.value.load();
        let bytes = value.to_le_bytes();
        if offset < bytes.len() {
            let read_len = buf.len().min(bytes.len() - offset);
            buf[0..read_len].copy_from_slice(&bytes[offset..offset + read_len]);
            Ok(read_len)
        } else {
            Ok(0)
        }
    }

    fn read_size(&self) -> usize {
        6
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        let value = TimeDifference::from_le_bytes(data.try_into().map_err(|_| {
            if data.len() < 6 {
                AbortCode::DataTypeMismatchLengthLow
            } else {
                AbortCode::DataTypeMismatchLengthHigh
            }
        })?);
        self.value.store(value);
        Ok(())
    }
}

impl ScalarField<TimeDifference> {
    /// Create a new ScalarField with the given value
    pub const fn new(value: TimeDifference) -> Self {
        Self {
            value: AtomicCell::new(value),
        }
    }
}

impl SubObjectAccess for ScalarField<TimeOfDay> {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let value = self.value.load();
        let bytes = value.to_le_bytes();
        if offset < bytes.len() {
            let read_len = buf.len().min(bytes.len() - offset);
            buf[0..read_len].copy_from_slice(&bytes[offset..offset + read_len]);
            Ok(read_len)
        } else {
            Ok(0)
        }
    }

    fn read_size(&self) -> usize {
        6
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        let value = TimeOfDay::from_le_bytes(data.try_into().map_err(|_| {
            if data.len() < 6 {
                AbortCode::DataTypeMismatchLengthLow
            } else {
                AbortCode::DataTypeMismatchLengthHigh
            }
        })?);
        self.value.store(value);
        Ok(())
    }
}

impl ScalarField<TimeOfDay> {
    /// Create a new ScalarField with the given value
    pub const fn new(value: TimeOfDay) -> Self {
        Self {
            value: AtomicCell::new(value),
        }
    }
}

/// A sub object which contains a fixed-size byte array
///
/// This is the data storage backing for all string types
#[allow(clippy::len_without_is_empty, missing_debug_implementations)]
pub struct ByteField<const N: usize> {
    value: UnsafeCell<[u8; N]>,
    write_offset: AtomicCell<Option<usize>>,
}

unsafe impl<const N: usize> Sync for ByteField<N> {}

impl<const N: usize> ByteField<N> {
    /// Create a new ByteField with the provided value
    pub const fn new(value: [u8; N]) -> Self {
        Self {
            value: UnsafeCell::new(value),
            write_offset: AtomicCell::new(None),
        }
    }

    /// Get the size of the ByteField
    pub fn len(&self) -> usize {
        N
    }

    /// Atomically store a new value to the sub object
    pub fn store(&self, value: [u8; N]) {
        // Any ongoing partial write will be cancelled
        critical_section::with(|cs| {
            self.write_offset.borrow(cs).set(None);
            let bytes = unsafe { &mut *self.value.get() };
            bytes.copy_from_slice(&value);
        });
    }

    /// Atomically read the value of the sub object
    pub fn load(&self) -> [u8; N] {
        critical_section::with(|_| unsafe { *self.value.get() })
    }
}

impl<const N: usize> Default for ByteField<N> {
    fn default() -> Self {
        Self {
            value: UnsafeCell::new([0; N]),
            write_offset: AtomicCell::new(None),
        }
    }
}

impl<const N: usize> SubObjectAccess for ByteField<N> {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        critical_section::with(|_| {
            let bytes = unsafe { &*self.value.get() };
            if bytes.len() > offset {
                let read_len = buf.len().min(bytes.len() - offset);
                buf[..read_len].copy_from_slice(&bytes[offset..offset + read_len]);
                Ok(read_len)
            } else {
                Ok(0)
            }
        })
    }

    fn read_size(&self) -> usize {
        N
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        critical_section::with(|_| {
            let bytes = unsafe { &mut *self.value.get() };
            if data.len() > bytes.len() {
                return Err(AbortCode::DataTypeMismatchLengthHigh);
            }
            bytes[..data.len()].copy_from_slice(data);
            Ok(())
        })
    }

    fn begin_partial(&self) -> Result<(), AbortCode> {
        self.write_offset.store(Some(0));
        Ok(())
    }

    fn write_partial(&self, buf: &[u8]) -> Result<(), AbortCode> {
        // Unwrap: fetch_update can only fail if the closure returns None
        let offset = self
            .write_offset
            .fetch_update(|old| Some(old.map(|x| x + buf.len())))
            .unwrap();
        if offset.is_none() {
            return Err(AbortCode::GeneralError);
        }
        let offset = offset.unwrap();
        if offset + buf.len() > N {
            return Err(AbortCode::DataTypeMismatchLengthHigh);
        }
        critical_section::with(|_| {
            let bytes = unsafe { &mut *self.value.get() };
            bytes[offset..offset + buf.len()].copy_from_slice(buf);
        });
        Ok(())
    }

    fn end_partial(&self) -> Result<(), AbortCode> {
        // No finalization action needed for byte fields
        self.write_offset.store(None);
        Ok(())
    }
}

/// A byte field which supports storing short values using null termination to indicate size
///
/// This is here to support VisibleString and UnicodeString types.
#[allow(clippy::len_without_is_empty, missing_debug_implementations)]
pub struct NullTermByteField<const N: usize>(ByteField<N>);

impl<const N: usize> NullTermByteField<N> {
    /// Create a new NullTermByteField with the provided value
    pub const fn new(value: [u8; N]) -> Self {
        Self(ByteField::new(value))
    }

    /// Return the size of the sub object
    pub fn len(&self) -> usize {
        N
    }

    /// Atomically load the value stored in the object
    ///
    /// Note that this will return the entire array, including any invalid bytes after the null
    /// terminator.
    pub fn load(&self) -> [u8; N] {
        self.0.load()
    }

    /// Atomically store a new value to the object
    pub fn store(&self, value: [u8; N]) {
        self.0.store(value);
    }

    /// Store a str to the object
    ///
    /// If the string is shorter than the object size, it will be stored with a null terminator
    /// If longer, an error will be returned.
    pub fn set_str(&self, value: &[u8]) -> Result<(), AbortCode> {
        self.0.begin_partial()?;
        self.0.write_partial(value)?;
        if value.len() < N {
            self.0.write_partial(&[0])?;
        }
        self.end_partial()?;
        Ok(())
    }
}

impl<const N: usize> Default for NullTermByteField<N> {
    fn default() -> Self {
        Self(ByteField::default())
    }
}

impl<const N: usize> SubObjectAccess for NullTermByteField<N> {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let size = self.0.read(offset, buf)?;
        let size = buf[0..size].iter().position(|b| *b == 0).unwrap_or(size);
        Ok(size)
    }

    fn read_size(&self) -> usize {
        critical_section::with(|_| {
            let bytes = unsafe { &*self.0.value.get() };
            // Find the first 0, or if there are none the length is the full array
            bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())
        })
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        self.0.begin_partial()?;
        self.0.write_partial(data)?;
        if data.len() < N {
            self.0.write_partial(&[0])?;
        }
        self.0.end_partial()?;
        Ok(())
    }

    fn begin_partial(&self) -> Result<(), AbortCode> {
        self.0.begin_partial()
    }

    fn write_partial(&self, data: &[u8]) -> Result<(), AbortCode> {
        self.0.write_partial(data)
    }

    fn end_partial(&self) -> Result<(), AbortCode> {
        // Null terminate if the length of data written is less than the sub object size
        if self.0.write_offset.load().unwrap_or(0) < N {
            self.0.write_partial(&[0])?;
        }
        self.0.end_partial()
    }
}

/// A sub-object implementation which is backed by a static byte slice
#[derive(Clone, Copy, Debug)]
pub struct ConstByteRefField {
    value: &'static [u8],
}

impl ConstByteRefField {
    /// Create a new const byteref field
    pub const fn new(value: &'static [u8]) -> Self {
        Self { value }
    }
}

impl SubObjectAccess for ConstByteRefField {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let read_len = buf.len().min(self.value.len() - offset);
        buf[..read_len].copy_from_slice(&self.value[offset..offset + read_len]);
        Ok(read_len)
    }

    fn read_size(&self) -> usize {
        self.value.len()
    }

    fn write(&self, _data: &[u8]) -> Result<(), AbortCode> {
        Err(AbortCode::ReadOnly)
    }
}

#[derive(Debug)]
/// A struct for a constant sub object whose value never changes
///
/// For simplicity, the value is stored directly as bytes, so use `to_le_bytes` when creating the
/// const object.
pub struct ConstField<const N: usize> {
    bytes: [u8; N],
}

impl<const N: usize> ConstField<N> {
    /// Create a const field
    pub const fn new(bytes: [u8; N]) -> Self {
        Self { bytes }
    }
}

impl<const N: usize> SubObjectAccess for ConstField<N> {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        if offset < self.bytes.len() {
            let read_len = buf.len().min(self.bytes.len() - offset);
            buf[..read_len].copy_from_slice(&self.bytes[offset..offset + read_len]);
            Ok(read_len)
        } else {
            Ok(0)
        }
    }

    fn read_size(&self) -> usize {
        N
    }

    fn write(&self, _data: &[u8]) -> Result<(), AbortCode> {
        Err(AbortCode::ReadOnly)
    }
}

/// A handler-backed sub-object for runtime registered implementation
#[allow(missing_debug_implementations)]
pub struct CallbackSubObject {
    handler: AtomicCell<Option<&'static dyn SubObjectAccess>>,
}

impl Default for CallbackSubObject {
    fn default() -> Self {
        Self::new()
    }
}

impl CallbackSubObject {
    /// Create a new object
    pub const fn new() -> Self {
        Self {
            handler: AtomicCell::new(None),
        }
    }

    /// Register a handler for this sub object
    pub fn register_handler(&self, handler: &'static dyn SubObjectAccess) {
        self.handler.store(Some(handler));
    }
}

impl SubObjectAccess for CallbackSubObject {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        if let Some(handler) = self.handler.load() {
            handler.read(offset, buf)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn read_size(&self) -> usize {
        if let Some(handler) = self.handler.load() {
            handler.read_size()
        } else {
            0
        }
    }

    fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
        if let Some(handler) = self.handler.load() {
            handler.write(data)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn begin_partial(&self) -> Result<(), AbortCode> {
        if let Some(handler) = self.handler.load() {
            handler.begin_partial()
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn write_partial(&self, buf: &[u8]) -> Result<(), AbortCode> {
        if let Some(handler) = self.handler.load() {
            handler.write_partial(buf)
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }

    fn end_partial(&self) -> Result<(), AbortCode> {
        if let Some(handler) = self.handler.load() {
            handler.end_partial()
        } else {
            Err(AbortCode::ResourceNotAvailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use zencan_common::object_model::ObjectCode;

    use crate::object_dict::{ObjectAccess, ProvidesSubObjects, SubInfo};

    use super::*;

    #[derive(Default)]
    struct ExampleRecord {
        val1: ScalarFieldU32,
        val2: ScalarFieldBool,
        val3: NullTermByteField<10>,
    }

    impl ProvidesSubObjects for ExampleRecord {
        fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
            match sub {
                0 => Some((
                    SubInfo::MAX_SUB_NUMBER,
                    const { &ConstField::new(3u8.to_le_bytes()) },
                )),
                1 => Some((SubInfo::new_u32().rw_access(), &self.val1)),
                2 => Some((SubInfo::new_u8().rw_access(), &self.val2)),
                3 => Some((
                    SubInfo::new_visible_str(self.val3.len()).rw_access(),
                    &self.val3,
                )),
                _ => None,
            }
        }

        fn object_code(&self) -> ObjectCode {
            ObjectCode::Record
        }
    }

    #[test]
    fn test_record_with_provides_sub_objects() {
        let record = ExampleRecord::default();

        assert_eq!(3, record.read_u8(0).unwrap());
        record.write(1, &42u32.to_le_bytes()).unwrap();
        assert_eq!(42, record.read_u32(1).unwrap());

        record.begin_partial(3).unwrap();
        // Do a write of the full length of the byte field
        record
            .write_partial(3, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9])
            .unwrap();
        let mut buf = [0; 10];
        record.read(3, 0, &mut buf).unwrap();
        assert_eq!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9], buf);
        // Do a write smaller than the size, and make sure it gets null terminated
        record.begin_partial(3).unwrap();
        record.write_partial(3, &[0, 1, 2, 3]).unwrap();
        record.write_partial(3, &[4, 5, 6, 7]).unwrap();
        record.end_partial(3).unwrap();
        let mut buf = [0; 9];
        record.read(3, 0, &mut buf).unwrap();
        assert_eq!([0u8, 1, 2, 3, 4, 5, 6, 7, 0], buf)
    }

    fn sub_read_test_helper(field: &dyn SubObjectAccess, expected_bytes: &[u8]) {
        let n = expected_bytes.len();

        assert_eq!(n, field.read_size());

        // Do an exact length read from offset 0
        let mut read_buf = vec![0xffu8; n + 10];
        let read_size = field.read(0, &mut read_buf).unwrap();
        assert_eq!(n, read_size);
        assert_eq!(expected_bytes, &read_buf[0..n]);

        // Do a long read
        let mut read_buf = vec![0xffu8; n + 10];
        let read_size = field.read(0, &mut read_buf).unwrap();
        assert_eq!(n, read_size);
        assert_eq!(expected_bytes, &read_buf[0..n]);

        if n > 2 {
            // Do a long read with offset
            let mut read_buf = vec![0xffu8; n + 10];
            let read_size = field.read(2, &mut read_buf).unwrap();
            assert_eq!(n - 2, read_size);
            assert_eq!(&expected_bytes[2..], &read_buf[0..n - 2]);

            // Do a short read with offset
            let mut read_buf = vec![0xffu8; n - 2];
            let read_size = field.read(1, &mut read_buf).unwrap();
            assert_eq!(n - 2, read_size);
            assert_eq!(expected_bytes[1..n - 1], read_buf);
        } else {
            let mut read_buf = vec![0xffu8; n + 10];
            let read_size = field.read(1, &mut read_buf).unwrap();
            assert_eq!(n.saturating_sub(1), read_size);
            assert_eq!(&expected_bytes[1..], &read_buf[0..read_size]);
        }
    }

    #[test]
    fn test_scalar_field_bool() {
        let field = ScalarFieldBool::new(false);
        field.store(true);
        assert_eq!(1, field.read_size());

        let mut read_buf = [0xffu8; 1];
        let read_size = field.read(0, &mut read_buf).unwrap();
        assert_eq!(1, read_size);
        assert_eq!([1], read_buf);
    }

    #[test]
    fn test_scalar_field_u8() {
        let field = ScalarFieldU8::new(42u8);
        let exp_bytes = 42u8.to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_u32() {
        let field = ScalarFieldU32::new(42u32);
        let exp_bytes = 42u32.to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_u16() {
        let field = ScalarFieldU16::new(42u16);
        let exp_bytes = 42u16.to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_u24() {
        let field = ScalarFieldU24::new(u24::new(42));
        let exp_bytes = u24::new(42).to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_u64() {
        let field = ScalarFieldU64::new(42u64);
        let exp_bytes = 42u64.to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_i8() {
        let field = ScalarFieldI8::new(-42i8);
        let exp_bytes = (-42i8).to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_i16() {
        let field = ScalarFieldI16::new(-42i16);
        let exp_bytes = (-42i16).to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_i24() {
        let field = ScalarFieldI24::new(i24::new(-42));
        let exp_bytes = i24::new(-42).to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_i32() {
        let field = ScalarFieldI32::new(-42i32);
        let exp_bytes = (-42i32).to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_i64() {
        let field = ScalarFieldI64::new(-42i64);
        let exp_bytes = (-42i64).to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_f32() {
        let field = ScalarFieldF32::new(42.5f32);
        let exp_bytes = 42.5f32.to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_f64() {
        let field = ScalarFieldF64::new(42.5f64);
        let exp_bytes = 42.5f64.to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_time_difference() {
        let value = TimeDifference::new(42, 1234);
        let field = ScalarField::<TimeDifference>::new(value);
        let exp_bytes = value.to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_scalar_field_time_of_day() {
        let value = TimeOfDay::new(42, 1234);
        let field = ScalarField::<TimeOfDay>::new(value);
        let exp_bytes = value.to_le_bytes();
        sub_read_test_helper(&field, &exp_bytes);
    }

    #[test]
    fn test_byte_field() {
        const N: usize = 10;
        let field = ByteField::new([0; N]);

        let write_data = Vec::from_iter(0u8..N as u8);
        field.write(&write_data).unwrap();

        sub_read_test_helper(&field, &write_data);
    }

    #[test]
    fn test_null_term_byte_field() {
        let field = NullTermByteField::new([0; 10]);
        // Write a full length value
        field.write(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]).unwrap();
        sub_read_test_helper(&field, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        // Write a short value
        field.write(&[1, 2, 3, 4]).unwrap();
        sub_read_test_helper(&field, &[1, 2, 3, 4]);
    }

    #[test]
    fn test_const_field() {
        let field = ConstField::new([1, 2, 3, 4, 5]);
        sub_read_test_helper(&field, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_const_byte_ref_field() {
        let field = ConstByteRefField::new(&[1, 2, 3, 4, 5]);
        sub_read_test_helper(&field, &[1, 2, 3, 4, 5]);
    }
}
