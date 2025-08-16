//! Collection of generic fields which implement a sub-object

use core::cell::UnsafeCell;

use zencan_common::{sdo::AbortCode, AtomicCell};

/// Allow transparent byte level access to a sub object
pub trait SubObjectAccess: Sync + Send {
    /// Read data from the sub object
    ///
    /// Read `buf.len()` bytes, starting at offset
    ///
    /// All sub objects are required to support partial read
    ///
    /// # Errors
    ///
    /// - [`AbortCode::DataTypeMismatchLengthHigh`] if `offset` + `buf.len()` exceeds the object
    ///   size
    /// - [`AbortCode::WriteOnly`] if the sub object does not support reading
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode>;

    /// Return the amount of data which can be read
    fn read_size(&self) -> usize;

    /// Write data to the sub object
    ///
    /// For most objects, the length of data must match the size of the object exactly. However, for
    /// some objects, such as Domain, VisibleString, or UnicodeString, or objects with custom
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
    /// order to allow transfer of the data in multiple blocks. It is up to the application to
    /// ensure that no other writes occur while a partial write is in progress, or else the object
    /// data may be corrupted and/or a call to `write_partial` may return an abort code on a
    /// subsequent call.
    ///
    /// Partial writes should always include the following, in this order:
    /// - One call to `begin_partial`
    /// - N calls to `write_partial`
    /// - One call to `end_partial`
    ///
    /// # Errors
    ///
    /// - [`AbortCode::UnsupportedAccess`] when the object does not support partial writes.
    fn begin_partial(&self) -> Result<(), AbortCode> {
        Err(AbortCode::UnsupportedAccess)
    }

    /// Write part of multi-part data to the object
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
    ($rust_type: ty, $data_type: ty) => {
        impl ScalarField<$rust_type> {
            /// Create a new ScalarField with the given value
            pub const fn new(value: $rust_type) -> Self {
                Self {
                    value: AtomicCell::new(value),
                }
            }
        }
        impl SubObjectAccess for ScalarField<$rust_type> {
            fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
                let bytes = self.value.load().to_le_bytes();
                if offset < bytes.len() {
                    let read_len = buf.len().min(bytes.len() - offset);
                    buf[0..read_len].copy_from_slice(&bytes[offset..offset + read_len]);
                    Ok(read_len)
                } else {
                    Ok(0)
                }
            }

            fn read_size(&self) -> usize {
                core::mem::size_of::<$rust_type>()
            }

            fn write(&self, data: &[u8]) -> Result<(), AbortCode> {
                let value = <$rust_type>::from_le_bytes(data.try_into().map_err(|_| {
                    if data.len() < size_of::<$rust_type>() {
                        AbortCode::DataTypeMismatchLengthLow
                    } else {
                        AbortCode::DataTypeMismatchLengthHigh
                    }
                })?);
                self.value.store(value);
                Ok(())
            }
        }
    };
}

impl_scalar_field!(u8, DataType::UInt8);
impl_scalar_field!(u16, DataType::UInt16);
impl_scalar_field!(u32, DataType::UInt32);
impl_scalar_field!(i8, DataType::Int8);
impl_scalar_field!(i16, DataType::Int16);
impl_scalar_field!(i32, DataType::Int32);
impl_scalar_field!(f32, DataType::Float);

// bool doesn't support from_le_bytes so it needs a special implementation
impl SubObjectAccess for ScalarField<bool> {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, AbortCode> {
        let value = self.value.load();
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
        self.value.store(value);
        Ok(())
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
        self.write_offset.store(None);
        critical_section::with(|_| {
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
        println!("Writing bytefield {} bytes", buf.len());
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
    use zencan_common::objects::{ObjectCode, SubInfo};

    use crate::object_dict::{ObjectAccess, ProvidesSubObjects};

    use super::*;

    #[derive(Default)]
    struct ExampleRecord {
        val1: ScalarField<u32>,
        val2: ScalarField<bool>,
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
                    SubInfo::new_visibile_str(self.val3.len()).rw_access(),
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

        assert!(n > 2, "Expected bytes cannot be shorted than 2 bytes");

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
    }

    #[test]
    fn test_scalar_field() {
        let field = ScalarField::<u32>::new(42u32);

        let exp_bytes = 42u32.to_le_bytes();

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
