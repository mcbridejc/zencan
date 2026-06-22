//! ReadSize trait for determining object sizes from rust types

use arbitrary_int::{i24, u24};

/// A trait for computing the read size of some types
pub trait ReadSize {
    /// Read size of the type
    const READ_SIZE: usize;
}

macro_rules! impl_read_size_builtin {
    ($($rust_type:ty),+ $(,)?) => {
        $(
            impl ReadSize for $rust_type {
                const READ_SIZE: usize = core::mem::size_of::<$rust_type>();
            }
        )+
    };
}
impl_read_size_builtin!(bool, u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

macro_rules! impl_read_size_arbitrary {
    ($($rust_type:ty),+ $(,)?) => {
        $(
            impl ReadSize for $rust_type {
                const READ_SIZE: usize = <$rust_type>::BITS.div_ceil(8);
            }
        )+
    };
}
impl_read_size_arbitrary!(u24, i24);
