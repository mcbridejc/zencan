use super::{ObjectGenerator, ObjectInstance};
use crate::{
    errors::CompileError,
    object_build_spec::{ObjectBuildSpec, SystemObjectKind},
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Runtime-provided objects share allocation/lookup generation with custom objects.
pub struct SystemObjectGenerator {
    pub spec: ObjectBuildSpec,
    pub kind: SystemObjectKind,
}

impl SystemObjectGenerator {
    pub fn new(spec: ObjectBuildSpec, kind: SystemObjectKind) -> Self {
        Self { spec, kind }
    }
}

impl ObjectGenerator for SystemObjectGenerator {
    fn type_definitions(&self) -> Result<TokenStream, CompileError> {
        Ok(quote!())
    }

    fn instance(&self) -> Result<Option<ObjectInstance>, CompileError> {
        let field_name = format_ident!("object{:04x}", self.spec.index);
        let object_type;
        let initializer;
        let reset;
        match &self.kind {
            SystemObjectKind::StorageCommand => {
                object_type = quote!(StorageCommandObject);
                initializer = quote!(StorageCommandObject::new(node_state.storage_context()));
                reset = Some(quote!(reset_object()));
            }
            SystemObjectKind::Identity {
                vendor,
                product,
                revision,
            } => {
                object_type = quote!(zencan_node::IdentityObject);
                initializer = quote!(zencan_node::IdentityObject::new(
                    #vendor, #product, #revision, 0
                ));
                reset = None;
            }
            SystemObjectKind::BootloaderInfo {
                application,
                num_sections,
            } => {
                let count = *num_sections as u8;
                object_type = quote!(zencan_node::BootloaderInfo<#application, #count>);
                initializer = quote!(zencan_node::BootloaderInfo::new());
                reset = Some(quote!(reset_object()));
            }
            SystemObjectKind::BootloaderSection { name, size } => {
                object_type = quote!(zencan_node::BootloaderSection);
                initializer = quote!(zencan_node::BootloaderSection::new(#name, #size));
                reset = None;
            }
            SystemObjectKind::Pdo => {
                let index = self.spec.index;
                let (pdos, base) = if index < 0x1800 {
                    (quote!(rpdos), 0x1400)
                } else {
                    (quote!(tpdos), 0x1800)
                };
                let mapping = index >= base + 0x200;
                let number = (index - base - if mapping { 0x200 } else { 0 }) as usize;
                let ty = if mapping {
                    quote!(PdoMappingObject)
                } else {
                    quote!(PdoCommObject)
                };
                object_type = quote!(#ty<'static>);
                initializer = quote!(#ty::new(&#pdos[#number]));
                // PDO reset has special handling in codegen
                reset = None;
            }
        };
        Ok(Some(ObjectInstance {
            field_name,
            object_type,
            initializer,
            reset,
        }))
    }
}
