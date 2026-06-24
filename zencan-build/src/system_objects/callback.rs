use super::{ObjectGenerator, ObjectInstance};
use crate::{
    errors::CompileError,
    object_build_spec::{ObjectBuildShape, ObjectBuildSpec},
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate a placeholder object for an application to register its own handle
pub struct CallbackObjectGenerator {
    pub spec: ObjectBuildSpec,
}

impl CallbackObjectGenerator {
    pub fn new(spec: ObjectBuildSpec) -> Self {
        Self { spec }
    }
}

impl ObjectGenerator for CallbackObjectGenerator {
    fn type_definitions(&self) -> Result<TokenStream, CompileError> {
        Ok(quote!())
    }

    fn instance(&self) -> Result<Option<ObjectInstance>, CompileError> {
        let code = match self.spec.shape {
            ObjectBuildShape::Var(_) => quote!(Var),
            ObjectBuildShape::Array(_) => quote!(Array),
            ObjectBuildShape::Record(_) => quote!(Record),
        };
        let field_name = format_ident!("object{:04x}", self.spec.index);
        let object_type = quote!(CallbackObject<'static>);
        let initializer =
            quote!(CallbackObject::new(zencan_node::common::object_model::ObjectCode::#code));

        Ok(Some(ObjectInstance {
            field_name,
            object_type,
            initializer,
            reset: None,
        }))
    }
}
