//! Per-object generation from the elaborated build specification.
use crate::errors::CompileError;
use proc_macro2::TokenStream;
use syn::Ident;

mod callback;
mod custom;
mod system;
pub use callback::*;
pub use custom::*;
pub use system::*;

pub struct ObjectInstance {
    pub field_name: Ident,
    pub object_type: TokenStream,
    pub initializer: TokenStream,
    pub reset: Option<TokenStream>,
    /// The initializer is a constant expression, so the object is a plain `static`, needing no
    /// initialization or reset at run time
    pub constant: bool,
}

pub trait ObjectGenerator {
    fn type_definitions(&self) -> Result<TokenStream, CompileError>;
    fn instance(&self) -> Result<Option<ObjectInstance>, CompileError>;
}
