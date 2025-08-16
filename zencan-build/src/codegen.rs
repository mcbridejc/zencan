use crate::errors::CompileError;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use zencan_common::device_config::{
    DataType as DCDataType, DefaultValue, DeviceConfig, Object, ObjectDefinition, PdoMapping,
    SubDefinition,
};
use zencan_common::objects::{AccessType, ObjectCode};

fn get_sub_field_name(sub: &SubDefinition) -> Result<syn::Ident, CompileError> {
    match &sub.field_name {
        Some(field_name) => {
            // Validate that the given field name is a valid rust identifier
            match syn::parse_str::<syn::Ident>(field_name) {
                Ok(ident) => Ok(ident),
                Err(_) => Err(CompileError::InvalidFieldName {
                    field_name: field_name.clone(),
                }),
            }
        }
        None => {
            // Unwrap safety: This should always yield a valid identifier
            Ok(syn::parse_str(&format!("sub{:x}", sub.sub_index)).unwrap())
        }
    }
}

/// Get the struct attribute type used to store this type
fn get_storage_type(data_type: DCDataType) -> (syn::Type, usize) {
    match data_type {
        DCDataType::Boolean => (syn::parse_quote!(ScalarField<bool>), 1),
        DCDataType::Int8 => (syn::parse_quote!(ScalarField<i8>), 1),
        DCDataType::Int16 => (syn::parse_quote!(ScalarField<i16>), 2),
        DCDataType::Int32 => (syn::parse_quote!(ScalarField<i32>), 4),
        DCDataType::UInt8 => (syn::parse_quote!(ScalarField<u8>), 1),
        DCDataType::UInt16 => (syn::parse_quote!(ScalarField<u16>), 2),
        DCDataType::UInt32 => (syn::parse_quote!(ScalarField<u32>), 4),
        DCDataType::Real32 => (syn::parse_quote!(ScalarField<f32>), 4),
        DCDataType::VisibleString(n) | DCDataType::UnicodeString(n) => (
            syn::parse_str(&format!("NullTermByteField::<{}>", n)).unwrap(),
            n,
        ),
        DCDataType::OctetString(n) => (syn::parse_str(&format!("ByteField::<{}>", n)).unwrap(), n),
        DCDataType::Domain => (syn::parse_quote!(CallbackSubObject), 0),
        _ => panic!("Unsupported data type {:?}", data_type),
    }
}

fn get_rust_type_and_size(data_type: DCDataType) -> (syn::Type, usize) {
    match data_type {
        DCDataType::Boolean => (syn::parse_quote!(bool), 1),
        DCDataType::Int8 => (syn::parse_quote!(i8), 1),
        DCDataType::Int16 => (syn::parse_quote!(i16), 2),
        DCDataType::Int32 => (syn::parse_quote!(i32), 4),
        DCDataType::UInt8 => (syn::parse_quote!(u8), 1),
        DCDataType::UInt16 => (syn::parse_quote!(u16), 2),
        DCDataType::UInt32 => (syn::parse_quote!(u32), 4),
        DCDataType::Real32 => (syn::parse_quote!(f32), 4),
        DCDataType::VisibleString(n)
        | DCDataType::OctetString(n)
        | DCDataType::UnicodeString(n) => (syn::parse_str(&format!("[u8; {}]", n)).unwrap(), n),
        DCDataType::Domain => (syn::parse_quote!(None), 0),
        _ => panic!("Unsupported data type {:?}", data_type),
    }
}

#[allow(dead_code)]
fn object_code_to_tokens(obj_code: ObjectCode) -> TokenStream {
    match obj_code {
        ObjectCode::Null => quote!(zencan_node::common::objects::ObjectCode::Null),
        ObjectCode::Record => quote!(zencan_node::common::objects::ObjectCode::Record),
        ObjectCode::Array => quote!(zencan_node::common::objects::ObjectCode::Array),
        ObjectCode::Var => quote!(zencan_node::common::objects::ObjectCode::Var),
        ObjectCode::Domain => quote!(zencan_node::common::objects::ObjectCode::Domain),
        ObjectCode::DefType => quote!(zencan_node::common::objects::ObjectCode::DefType),
        ObjectCode::DefStruct => quote!(zencan_node::common::objects::ObjectCode::DefStruct),
    }
}

/// Convert an AccessType enum to a tokenstream expressing the variant
fn access_type_to_tokens(at: AccessType) -> TokenStream {
    match at {
        AccessType::Ro => quote!(zencan_node::common::objects::AccessType::Ro),
        AccessType::Wo => quote!(zencan_node::common::objects::AccessType::Wo),
        AccessType::Rw => quote!(zencan_node::common::objects::AccessType::Rw),
        AccessType::Const => quote!(zencan_node::common::objects::AccessType::Const),
    }
}

fn data_type_to_tokens(dt: DCDataType) -> TokenStream {
    match dt {
        DCDataType::Boolean => quote!(zencan_node::common::objects::DataType::Boolean),
        DCDataType::Int8 => quote!(zencan_node::common::objects::DataType::Int8),
        DCDataType::Int16 => quote!(zencan_node::common::objects::DataType::Int16),
        DCDataType::Int32 => quote!(zencan_node::common::objects::DataType::Int32),
        DCDataType::UInt8 => quote!(zencan_node::common::objects::DataType::UInt8),
        DCDataType::UInt16 => quote!(zencan_node::common::objects::DataType::UInt16),
        DCDataType::UInt32 => quote!(zencan_node::common::objects::DataType::UInt32),
        DCDataType::Real32 => quote!(zencan_node::common::objects::DataType::Real32),
        DCDataType::VisibleString(_) => {
            quote!(zencan_node::common::objects::DataType::VisibleString)
        }
        DCDataType::UnicodeString(_) => {
            quote!(zencan_node::common::objects::DataType::UnicodeString)
        }
        DCDataType::OctetString(_) => quote!(zencan_node::common::objects::DataType::OctetString),
        DCDataType::TimeOfDay => quote!(zencan_node::common::objects::DataType::TimeOfDay),
        DCDataType::TimeDifference => {
            quote!(zencan_node::common::objects::DataType::TimeDifference)
        }
        DCDataType::Domain => quote!(zencan_node::common::objects::DataType::Domain),
    }
}

fn pdo_mapping_to_tokens(p: PdoMapping) -> TokenStream {
    match p {
        PdoMapping::None => quote!(zencan_node::common::objects::PdoMapping::None),
        PdoMapping::Tpdo => quote!(zencan_node::common::objects::PdoMapping::Tpdo),
        PdoMapping::Rpdo => quote!(zencan_node::common::objects::PdoMapping::Rpdo),
        PdoMapping::Both => quote!(zencan_node::common::objects::PdoMapping::Both),
    }
}

/// Return true if any subobjects on the object support being mapped to a TPDO
fn object_supports_tpdo(obj: &ObjectDefinition) -> bool {
    match &obj.object {
        Object::Var(def) => def.pdo_mapping.supports_tpdo(),
        Object::Array(def) => def.pdo_mapping.supports_tpdo(),
        Object::Record(def) => def.subs.iter().any(|s| s.pdo_mapping.supports_tpdo()),
    }
}

fn string_to_byte_literal_tokens(s: &str, size: usize) -> Result<TokenStream, CompileError> {
    let b = s.as_bytes();
    if b.len() > size {
        return Err(CompileError::DefaultValueTooLong {
            message: format!("String {} is too long for type with length {}", s, size),
        });
    }
    let mut padded = vec![0u8; size];
    padded[..b.len()].copy_from_slice(b);

    Ok(quote!([#(#padded),*]))
}

fn generate_object_definition(obj: &ObjectDefinition) -> Result<TokenStream, CompileError> {
    if obj.application_callback {
        // Objects implemented in application callbacks do not generate a struct
        return Ok(quote! {});
    }
    let struct_name: syn::Ident = syn::parse_str(&format!("Object{:X}", obj.index)).unwrap();

    let mut field_tokens = TokenStream::new();
    let mut tpdo_mapping = false;
    let mut highest_sub_index = 0;
    match &obj.object {
        Object::Record(def) => {
            for sub in &def.subs {
                let field_name = get_sub_field_name(sub)?;
                let (field_type, _) = get_storage_type(sub.data_type);
                field_tokens.extend(quote! {
                    pub #field_name: #field_type,
                });
                tpdo_mapping |= sub.pdo_mapping.supports_tpdo();
                highest_sub_index = highest_sub_index.max(sub.sub_index);
            }
        }
        Object::Array(def) => {
            let (field_type, _) = get_storage_type(def.data_type);
            let array_size = def.array_size;
            field_tokens.extend(quote! {
                pub array: [#field_type; #array_size],
            });
            tpdo_mapping |= def.pdo_mapping.supports_tpdo();
            highest_sub_index = array_size as u8;
        }
        Object::Var(def) => {
            let (field_type, _) = get_storage_type(def.data_type);
            field_tokens.extend(quote! {
                pub value: #field_type,
            });
            tpdo_mapping |= def.pdo_mapping.supports_tpdo();
            highest_sub_index = 0;
        }
    }

    if tpdo_mapping {
        let n = (highest_sub_index as usize).div_ceil(8);
        field_tokens.extend(quote! {
            flags: ObjectFlags<#n>,
        });
    }

    Ok(quote! {
        #[allow(dead_code)]
        pub struct #struct_name {
            #field_tokens
        }
    })
}

/// Get DefaultValue for a given data type. This is the default value when none is provided.
fn default_default_value(data_type: DCDataType) -> DefaultValue {
    match data_type {
        DCDataType::Boolean
        | DCDataType::Int8
        | DCDataType::Int16
        | DCDataType::Int32
        | DCDataType::UInt8
        | DCDataType::UInt16
        | DCDataType::UInt32 => DefaultValue::Integer(0),
        DCDataType::Real32 => DefaultValue::Float(0.0),
        DCDataType::VisibleString(_)
        | DCDataType::UnicodeString(_)
        | DCDataType::OctetString(_) => DefaultValue::String("".to_string()),
        DCDataType::TimeOfDay => DefaultValue::String("".to_string()),
        DCDataType::TimeDifference => DefaultValue::String("".to_string()),
        DCDataType::Domain => DefaultValue::String("".to_string()),
    }
}

fn get_default_tokens(
    value: &DefaultValue,
    data_type: DCDataType,
) -> Result<TokenStream, CompileError> {
    if matches!(data_type, DCDataType::Domain) {
        return Ok(quote!(CallbackSubObject::new()));
    }
    match value {
        DefaultValue::String(s) => {
            if !data_type.is_str() {
                return Err(CompileError::DefaultValueTypeMismatch {
                    message: format!(
                        "Default value {} is not a string for type {:?}",
                        s, data_type
                    ),
                });
            }
            let byte_lit = string_to_byte_literal_tokens(s, data_type.size())?;
            // OctetStrings are always the exact length
            if matches!(data_type, DCDataType::OctetString(_)) {
                Ok(quote!(ByteField::new(#byte_lit)))
            } else {
                Ok(quote!(NullTermByteField::new(#byte_lit)))
            }
        }
        DefaultValue::Float(f) => match data_type {
            DCDataType::Real32 => Ok(quote!(ScalarField<f32>::new(#f))),
            _ => Err(CompileError::DefaultValueTypeMismatch {
                message: format!(
                    "Default value {} is not a valid value for type {:?}",
                    f, data_type
                ),
            }),
        },
        DefaultValue::Integer(i) => {
            // Create token as stream so the literal does not have an explicit type (e.g. '32' instead of '32i64')
            match data_type {
                DCDataType::Boolean => {
                    if *i != 0 {
                        Ok(quote!(ScalarField<bool>::new(true)))
                    } else {
                        Ok(quote!(ScalarField<bool>::new(false)))
                    }
                }
                DCDataType::Int8 => Ok(quote!(ScalarField::<i8>::new(#i as i8))),
                DCDataType::Int16 => Ok(quote!(ScalarField::<i16>::new(#i as i16))),
                DCDataType::Int32 => Ok(quote!(ScalarField::<i32>::new(#i as i32))),
                DCDataType::UInt8 => Ok(quote!(ScalarField::<u8>::new(#i as u8))),
                DCDataType::UInt16 => Ok(quote!(ScalarField::<u16>::new(#i as u16))),
                DCDataType::UInt32 => Ok(quote!(ScalarField::<u32>::new(#i as u32))),
                DCDataType::Real32 => Ok(quote!(ScalarField::<f32>::new(#i as f32))),
                _ => Err(CompileError::DefaultValueTypeMismatch {
                    message: format!(
                        "Default value {} is not a valid value for type {:?}",
                        i, data_type
                    ),
                }),
            }
        }
    }
}

fn get_object_impls(
    obj: &ObjectDefinition,
    struct_name: &syn::Ident,
) -> Result<TokenStream, CompileError> {
    let mut accessor_methods = TokenStream::new();
    let mut default_init_tokens = TokenStream::new();
    let mut get_sub_tokens = TokenStream::new();
    let mut flag_number = 0usize;
    let object_code;

    match &obj.object {
        Object::Var(def) => {
            let (field_type, size) = get_rust_type_and_size(def.data_type);
            let field_name = format_ident!("value");
            let setter_name = format_ident!("set_{}", field_name);
            let getter_name = format_ident!("get_{}", field_name);
            let data_type = data_type_to_tokens(def.data_type);
            let access_type = access_type_to_tokens(def.access_type.0);
            let pdo_mapping = pdo_mapping_to_tokens(def.pdo_mapping);
            let persist = def.persist;

            let default_value = def
                .default_value
                .clone()
                .unwrap_or(default_default_value(def.data_type));
            let default_value = get_default_tokens(&default_value, def.data_type)?;
            default_init_tokens.extend(quote! {
                #field_name: #default_value,
            });

            if def.pdo_mapping.supports_tpdo() {
                flag_number = 1;
            }

            // Accessors are generated for all data types, except Domain
            if !matches!(def.data_type, DCDataType::Domain) {
                accessor_methods.extend(quote! {
                    #[allow(dead_code)]
                    pub fn #setter_name(&self, value: #field_type) {
                        self.#field_name.store(value);
                    }

                    #[allow(dead_code)]
                    pub fn #getter_name(&self) -> #field_type {
                        self.#field_name.load()
                    }
                });
            }

            get_sub_tokens.extend(quote! {
                match sub {
                    0 => Some(
                        (SubInfo {
                            access_type: #access_type,
                            data_type: #data_type,
                            size: #size,
                            pdo_mapping: #pdo_mapping,
                            persist: #persist,
                        },
                        &self.value)
                    ),
                    _ => None
                }
            });

            object_code = quote!(zencan_node::common::objects::ObjectCode::Var);
        }

        Object::Array(def) => {
            let (field_type, storage_size) = get_rust_type_and_size(def.data_type);
            let array_size = def.array_size;
            let data_type = data_type_to_tokens(def.data_type);
            let access_type = access_type_to_tokens(def.access_type.0);
            let pdo_mapping = pdo_mapping_to_tokens(def.pdo_mapping);
            let persist = def.persist;

            let default_value =
                def.default_value
                    .clone()
                    .unwrap_or(vec![default_default_value(def.data_type); array_size]);

            let default_tokens: Vec<_> = default_value
                .iter()
                .map(|v| get_default_tokens(v, def.data_type))
                .collect::<Result<Vec<_>, CompileError>>()?;

            if !matches!(def.data_type, DCDataType::Domain) {
                accessor_methods.extend(quote! {
                    #[allow(dead_code)]
                    pub fn set(&self, idx: usize, value: #field_type) -> Result<(), AbortCode> {
                        if idx >= #array_size {
                            return Err(AbortCode::NoSuchSubIndex)
                        }
                        self.array[idx].store(value);
                        Ok(())
                    }
                    #[allow(dead_code)]
                    pub fn get(&self, idx: usize) -> Result<#field_type, AbortCode> {
                        if idx >= #array_size {
                            return Err(AbortCode::NoSuchSubIndex)
                        }
                        Ok(self.array[idx].load())
                    }
                });
            }

            default_init_tokens.extend(quote! {
                array: [#(#default_tokens),*],
            });

            get_sub_tokens.extend(quote! {
                if sub == 0 {
                    Some((
                        SubInfo::MAX_SUB_NUMBER,
                        const { &ConstField::new((#array_size as u8).to_le_bytes()) },
                    ))
                } else if sub as usize > #array_size {
                    return None;
                } else {
                    Some((SubInfo {
                        access_type: #access_type,
                        data_type: #data_type,
                        size: #storage_size,
                        pdo_mapping: #pdo_mapping,
                        persist: #persist,
                    }, &self.array[sub as usize - 1]))
                }
            });

            if def.pdo_mapping.supports_tpdo() {
                flag_number = array_size + 1;
            }

            object_code = quote!(zencan_node::common::objects::ObjectCode::Array);
        }

        Object::Record(def) => {
            let mut match_statements = TokenStream::new();

            // For records, sub0 gives the highest sub object support by the record
            let max_sub = def.subs.iter().map(|s| s.sub_index).max().unwrap_or(0);

            if object_supports_tpdo(obj) {
                flag_number = max_sub as usize + 1;
            }

            accessor_methods.extend(quote! {
                #[allow(dead_code)]
                pub fn get_sub0(&self) -> u8 {
                    #max_sub
                }
            });

            match_statements.extend(quote! {
                0 => {
                    Some(
                        (
                            SubInfo::MAX_SUB_NUMBER,
                            const { &ConstField::new(#max_sub.to_le_bytes()) },
                        )
                    )
                }
            });

            for sub in &def.subs {
                let field_name = get_sub_field_name(sub)?;
                let (field_type, size) = get_rust_type_and_size(sub.data_type);
                let setter_name = format_ident!("set_{}", field_name);
                let getter_name = format_ident!("get_{}", field_name);
                let sub_index = sub.sub_index;
                let data_type = data_type_to_tokens(sub.data_type);
                let pdo_mapping = pdo_mapping_to_tokens(sub.pdo_mapping);
                let persist = sub.persist;

                let default_value = sub
                    .default_value
                    .clone()
                    .unwrap_or(default_default_value(sub.data_type));
                let default_tokens = get_default_tokens(&default_value, sub.data_type)?;

                let access_type = access_type_to_tokens(sub.access_type.0);

                if !matches!(sub.data_type, DCDataType::Domain) {
                    accessor_methods.extend(quote! {
                        #[allow(dead_code)]
                        pub fn #setter_name(&self, value: #field_type) {
                            self.#field_name.store(value)
                        }
                        #[allow(dead_code)]
                        pub fn #getter_name(&self) -> #field_type {
                            self.#field_name.load()
                        }
                    });
                }
                match_statements.extend(quote! {
                    #sub_index => Some(
                        (
                            SubInfo {
                                access_type: #access_type,
                                data_type: #data_type,
                                size: #size,
                                pdo_mapping: #pdo_mapping,
                                persist: #persist,
                            },
                            &self.#field_name
                        )
                    ),
                });
                default_init_tokens.extend(quote! {
                    #field_name: #default_tokens,
                });
            }

            get_sub_tokens.extend(quote! {
                match sub {
                    #match_statements
                    _ => None,
                }
            });

            object_code = quote!(zencan_node::common::objects::ObjectCode::Record);
        }
    }

    let mut flag_method_tokens = TokenStream::new();
    let mut flag_default_tokens = TokenStream::new();
    if flag_number > 0 {
        let flag_size = (flag_number).div_ceil(8);
        flag_method_tokens.extend(quote! {
            fn flags(&self) -> Option<&dyn ObjectFlagAccess> {
                Some(&self.flags)
            }
        });
        flag_default_tokens.extend(quote! {
            flags: ObjectFlags::<#flag_size>::new(NODE_STATE.pdo_sync()),
        });
    }

    Ok(quote! {
        impl #struct_name {
            #accessor_methods

            const fn default() -> Self {
                #struct_name {
                    #default_init_tokens
                    #flag_default_tokens
                }
            }
        }

        impl ProvidesSubObjects for #struct_name {
            fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
                #get_sub_tokens
            }

            #flag_method_tokens

            fn object_code(&self) -> zencan_node::common::objects::ObjectCode {
                #object_code
            }
        }
    })
}

pub fn generate_object_code(
    obj: &ObjectDefinition,
    struct_name: &syn::Ident,
) -> Result<TokenStream, CompileError> {
    let struct_def = generate_object_definition(obj)?;
    let impls = get_object_impls(obj, struct_name)?;

    Ok(quote! {
        #struct_def
        #impls
    })
}

pub fn generate_state_inst(dev: &DeviceConfig) -> TokenStream {
    let n_rpdo = dev.pdos.num_rpdo as usize;
    let n_tpdo = dev.pdos.num_tpdo as usize;

    let mut tokens = TokenStream::new();

    if !dev.bootloader.sections.is_empty() {
        let num_sections = dev.bootloader.sections.len() as u8;
        let application = dev.bootloader.application;
        tokens.extend(quote! {
            pub static BOOTLOADER_INFO:
                zencan_node::BootloaderInfo<#application, #num_sections> =
                zencan_node::BootloaderInfo::new();
        });
        for (i, section) in dev.bootloader.sections.iter().enumerate() {
            let var_name = format_ident!("BOOTLOADER_SECTION{i}");
            let size: u32 = section.size;
            let section_name = &section.name;
            tokens.extend(quote! {
                pub static #var_name: zencan_node::BootloaderSection =
                    zencan_node::BootloaderSection::new(
                        #section_name,
                        #size
                    );
            })
        }
    }

    let tpdo_numbers = 0..n_tpdo;
    tokens.extend(quote! {
        pub static TPDO_COMM_OBJECTS: [PdoCommObject; #n_tpdo] = [
            #(PdoCommObject::new(&NODE_STATE.tpdos()[#tpdo_numbers])),*
        ];
    });
    let tpdo_numbers = 0..n_tpdo;
    tokens.extend(quote! {
        pub static TPDO_MAPPING_OBJECTS: [PdoMappingObject; #n_tpdo] = [
            #(PdoMappingObject::new(&OD_TABLE, &NODE_STATE.tpdos()[#tpdo_numbers])),*
        ];
    });
    let rpdo_numbers = 0..n_rpdo;
    tokens.extend(quote! {
        pub static RPDO_COMM_OBJECTS: [PdoCommObject; #n_rpdo] = [
            #(PdoCommObject::new(&NODE_STATE.rpdos()[#rpdo_numbers])),*
        ];
    });
    let rpdo_numbers = 0..n_rpdo;
    tokens.extend(quote! {
        pub static RPDO_MAPPING_OBJECTS: [PdoMappingObject; #n_rpdo] = [
            #(PdoMappingObject::new(&OD_TABLE, &NODE_STATE.rpdos()[#rpdo_numbers])),*
        ];
    });

    if dev.support_storage {
        tokens.extend(quote! {
            pub static STORAGE_COMMAND_OBJECT: StorageCommandObject =
                StorageCommandObject::new(&OD_TABLE, NODE_STATE.storage_context());
        });
    }

    tokens.extend(quote! {
        #[allow(static_mut_refs)]
        static mut SDO_BUFFER: [u8; SDO_BUFFER_SIZE] = [0; SDO_BUFFER_SIZE];
        pub static NODE_STATE: NodeState<#n_rpdo, #n_tpdo> = NodeState::new();
        #[allow(static_mut_refs)]
        pub static NODE_MBOX: NodeMbox = NodeMbox::new(NODE_STATE.rpdos(), unsafe { &mut SDO_BUFFER });
    });

    tokens
}

/// Generate code for a node from a [`DeviceConfig`] as a TokenStream
pub fn device_config_to_tokens(dev: &DeviceConfig) -> Result<TokenStream, CompileError> {
    let mut object_defs = TokenStream::new();
    let mut object_instantiations = TokenStream::new();
    let mut table_entries = TokenStream::new();

    let mut sorted_objects: Vec<&ObjectDefinition> = dev.objects.iter().collect();
    sorted_objects.sort_by_key(|o| o.index);

    for obj in &sorted_objects {
        let struct_name = format_ident!("Object{:X}", obj.index);
        let inst_name = format_ident!("OBJECT{:X}", obj.index);
        let index: syn::Lit = syn::parse_str(&format!("0x{:X}", obj.index)).unwrap();
        if obj.index == 0x1010 {
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &STORAGE_COMMAND_OBJECT,
                },
            });
        } else if obj.index == 0x5500 {
            // bootloader info object as usize
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &BOOTLOADER_INFO,
                },
            });
        } else if obj.index >= 0x5510 && obj.index <= 0x551f {
            let section = obj.index - 0x5510;
            let object_ident = format_ident!("BOOTLOADER_SECTION{}", section);
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &#object_ident,
                },
            });
        } else if obj.index >= 0x1400 && obj.index < 0x1600 {
            let n = obj.index as usize - 0x1400;
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &RPDO_COMM_OBJECTS[#n]
                },
            })
        } else if obj.index >= 0x1600 && obj.index < 0x1800 {
            let n = obj.index as usize - 0x1600;
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &RPDO_MAPPING_OBJECTS[#n]
                },
            })
        } else if obj.index >= 0x1800 && obj.index < 0x1A00 {
            let n = obj.index as usize - 0x1800;
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &TPDO_COMM_OBJECTS[#n]
                },
            })
        } else if obj.index >= 0x1A00 && obj.index < 0x1C00 {
            let n = obj.index as usize - 0x1A00;
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &TPDO_MAPPING_OBJECTS[#n]
                },
            })
        } else if !obj.application_callback {
            object_defs.extend(generate_object_code(obj, &struct_name)?);
            object_instantiations.extend(quote! {
                pub static #inst_name: #struct_name = #struct_name::default();
            });
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &#inst_name,
                },
            });
        } else {
            let object_code = object_code_to_tokens(obj.object_code());
            object_instantiations.extend(quote! {
                pub static #inst_name: CallbackObject = CallbackObject::new(&OD_TABLE, #object_code);
            });
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: &#inst_name,
                },
            });
        }
    }

    object_instantiations.extend(generate_state_inst(dev));

    let table_len = dev.objects.len();
    Ok(quote! {
        #[allow(unused_imports)]
        use zencan_node::common::AtomicCell;
        #[allow(unused_imports)]
        use core::cell::Cell;
        #[allow(unused_imports)]
        use core::cell::RefCell;
        #[allow(unused_imports)]
        use zencan_node::critical_section::Mutex;
        #[allow(unused_imports)]
        use zencan_node::common::objects::SubInfo;
        #[allow(unused_imports)]
        use zencan_node::common::sdo::AbortCode;
        #[allow(unused_imports)]
        use zencan_node::object_dict::{
            CallbackObject,
            CallbackSubObject,
            ObjectFlags,
            ODEntry,
            ObjectAccess,
            ProvidesSubObjects,
            SubObjectAccess,
            ObjectFlagAccess,
            ScalarField,
            ByteField,
            ConstField,
            NullTermByteField,
        };
        #[allow(unused_imports)]
        use zencan_node::SDO_BUFFER_SIZE;
        #[allow(unused_imports)]
        use zencan_node::pdo::{PdoCommObject, PdoMappingObject};
        #[allow(unused_imports)]
        use zencan_node::storage::StorageCommandObject;
        #[allow(unused_imports)]
        use zencan_node::NodeMbox;
        #[allow(unused_imports)]
        use zencan_node::{NodeState, NodeStateAccess};
        #object_defs
        #object_instantiations
        pub static OD_TABLE: [ODEntry; #table_len] = [
            #table_entries
        ];
    })
}

/// Generate code for a node from a [`DeviceConfig`] as a string
///
/// # Arguments
/// * `dev` - The device config
/// * `format` - If true, generated code will be formatted with `prettyplease`
pub fn device_config_to_string(dev: &DeviceConfig, format: bool) -> Result<String, CompileError> {
    let tokens = device_config_to_tokens(dev)?;

    if format {
        let parsed_file = match syn::parse_file(&tokens.to_string()) {
            Ok(f) => f,
            Err(e) => panic!("Error parsing generated code: {}", e),
        };
        Ok(prettyplease::unparse(&parsed_file))
    } else {
        Ok(tokens.to_string())
    }
}
