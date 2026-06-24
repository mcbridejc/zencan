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
}

pub trait ObjectGenerator {
    fn type_definitions(&self) -> Result<TokenStream, CompileError>;
    fn instance(&self) -> Result<Option<ObjectInstance>, CompileError>;
}
