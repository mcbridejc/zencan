use crate::errors::CompileError;
use crate::utils::{
    scalar_read_snippet, scalar_write_snippet, string_read_snippet, string_write_snippet,
};
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
        Object::Domain(_) => false,
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
                let (field_type, _) = get_rust_type_and_size(sub.data_type);
                field_tokens.extend(quote! {
                    pub #field_name: AtomicCell<#field_type>,
                });
                tpdo_mapping |= sub.pdo_mapping.supports_tpdo();
                highest_sub_index = highest_sub_index.max(sub.sub_index);
            }
        }
        Object::Array(def) => {
            let (field_type, _) = get_rust_type_and_size(def.data_type);
            let array_size = def.array_size;
            field_tokens.extend(quote! {
                pub size: u8,
                pub array: Mutex<RefCell<[#field_type; #array_size]>>,
            });
            tpdo_mapping |= def.pdo_mapping.supports_tpdo();
            highest_sub_index = array_size as u8;
        }
        Object::Var(def) => {
            let (field_type, _) = get_rust_type_and_size(def.data_type);
            field_tokens.extend(quote! {
                pub value: AtomicCell<#field_type>,
            });
            tpdo_mapping |= def.pdo_mapping.supports_tpdo();
            highest_sub_index = 0;
        }
        Object::Domain(_) => {
            panic!("Domain objects are only supported with application callback enabled")
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
        #[derive(Debug)]
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
            Ok(string_to_byte_literal_tokens(s, data_type.size())?)
        }
        DefaultValue::Float(f) => match data_type {
            DCDataType::Real32 => Ok(quote!(#f)),
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
                        Ok(quote!(true))
                    } else {
                        Ok(quote!(false))
                    }
                }
                DCDataType::Int8 => Ok(quote!(#i as i8)),
                DCDataType::Int16 => Ok(quote!(#i as i16)),
                DCDataType::Int32 => Ok(quote!(#i as i32)),
                DCDataType::UInt8 => Ok(quote!(#i as u8)),
                DCDataType::UInt16 => Ok(quote!(#i as u16)),
                DCDataType::UInt32 => Ok(quote!(#i as u32)),
                DCDataType::Real32 => Ok(quote!(#i as f32)),
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
    fn get_tpdo_event_snippet(max_sub: usize) -> TokenStream {
        quote! {
            fn set_event_flag(&self, sub: u8) -> Result<(), AbortCode> {
                if sub as usize > #max_sub {
                    return Err(AbortCode::NoSuchSubIndex);
                }
                self.flags.set_flag(sub);
                Ok(())
            }

            fn read_event_flag(&self, sub: u8) -> bool {
                self.flags.get_flag(sub)
            }

            fn clear_events(&self) {
                self.flags.clear();
            }
        }
    }

    match &obj.object {
        Object::Var(def) => {
            let (field_type, size) = get_rust_type_and_size(def.data_type);
            let field_name = format_ident!("value");
            let setter_name = format_ident!("set_{}", field_name);
            let getter_name = format_ident!("get_{}", field_name);
            let write_snippet;
            let read_snippet;
            if def.data_type.is_str() {
                write_snippet = string_write_snippet(&field_name, size);
                read_snippet = string_read_snippet(&field_name, size);
            } else {
                write_snippet = scalar_write_snippet(&field_name, &field_type);
                read_snippet = scalar_read_snippet(&field_name);
            }
            let data_type = data_type_to_tokens(def.data_type);
            let access_type = access_type_to_tokens(def.access_type.0);
            let pdo_mapping = pdo_mapping_to_tokens(def.pdo_mapping);
            let persist = def.persist;

            let default_value = def
                .default_value
                .clone()
                .unwrap_or(default_default_value(def.data_type));
            let default_tokens = get_default_tokens(&default_value, def.data_type)?;

            let mut tpdo_event_tokens = TokenStream::new();
            let mut tpdo_default_tokens = TokenStream::new();
            if def.pdo_mapping.supports_tpdo() {
                tpdo_event_tokens.extend(get_tpdo_event_snippet(0));
                tpdo_default_tokens.extend(quote! {
                    flags: ObjectFlags::<1>::new(NODE_STATE.pdo_sync()),
                });
            }

            Ok(quote! {
                #[allow(dead_code)]
                impl #struct_name {
                    pub fn #setter_name(&self, value: #field_type) {
                        self.#field_name.store(value);
                    }

                    pub fn #getter_name(&self) -> #field_type {
                        self.#field_name.load()
                    }

                    const fn default() -> Self {
                        #struct_name {
                            #field_name: AtomicCell::new(#default_tokens),
                            #tpdo_default_tokens
                        }
                    }
                }

                impl ObjectRawAccess for #struct_name {
                    fn write(&self, sub: u8, offset: usize, data: &[u8]) -> Result<(), AbortCode> {
                        if sub == 0 {
                            #write_snippet
                            Ok(())
                        } else {
                            Err(AbortCode::NoSuchSubIndex)
                        }
                    }
                    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<(), AbortCode> {
                        if sub == 0 {
                            #read_snippet
                            Ok(())
                        } else {
                            Err(AbortCode::NoSuchSubIndex)
                        }
                    }
                    fn sub_info(&self, sub: u8) -> Result<SubInfo, AbortCode> {
                        if sub != 0 {
                            return Err(AbortCode::NoSuchSubIndex);
                        }
                        Ok(SubInfo {
                            access_type: #access_type,
                            data_type: #data_type,
                            size: #size,
                            pdo_mapping: #pdo_mapping,
                            persist: #persist,
                        })
                    }
                    fn object_code(&self) -> zencan_node::common::objects::ObjectCode {
                        zencan_node::common::objects::ObjectCode::Var
                    }

                    #tpdo_event_tokens
                }
            })
        }

        Object::Array(def) => {
            let (field_type, storage_size) = get_rust_type_and_size(def.data_type);
            let array_size = def.array_size;
            let flag_size = (array_size + 1).div_ceil(8);
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

            let write_snippet;
            let read_snippet;
            if def.data_type.is_str() {
                write_snippet = quote! {
                    if offset + data.len() > #storage_size {
                        return Err(AbortCode::DataTypeMismatchLengthHigh);
                    }
                    zencan_node::critical_section::with(|cs| {
                        let mut array = self.array.borrow_ref_mut(cs);
                        array[(sub - 1) as usize][offset..offset + data.len()].copy_from_slice(data)
                    });
                };
                read_snippet = quote! {
                    if offset + data.len() > #storage_size {
                        return Err(AbortCode::DataTypeMismatchLengthHigh);
                    }
                    zencan_node::critical_section::with(|cs| {
                        let mut array = self.array.borrow_ref(cs);
                        buf.copy_from_slice(&array[(sub - 1) as usize][offset..offset + data.len()]);
                    })
                };
            } else {
                write_snippet = quote! {
                    if offset != 0 {
                        return Err(AbortCode::UnsupportedAccess);
                    }
                    let value = #field_type::from_le_bytes(data.try_into().map_err(|_| {
                        if data.len() < size_of::<#field_type>() {
                            AbortCode::DataTypeMismatchLengthLow
                        } else {
                            AbortCode::DataTypeMismatchLengthHigh
                        }
                    })?);
                    self.set((sub - 1) as usize, value)?;
                };
                read_snippet = quote! {
                    let value = self.get((sub - 1) as usize)?;
                    let bytes = value.to_le_bytes();
                    if offset + buf.len() > size_of::<#field_type>() {
                        return Err(AbortCode::DataTypeMismatchLengthHigh);
                    }
                    buf.copy_from_slice(&bytes[offset..offset+buf.len()]);
                }
            }

            let mut tpdo_event_tokens = TokenStream::new();
            let mut tpdo_default_tokens = TokenStream::new();
            if def.pdo_mapping.supports_tpdo() {
                tpdo_event_tokens.extend(get_tpdo_event_snippet(array_size));
                tpdo_default_tokens.extend(quote! {
                    flags: ObjectFlags::<#flag_size>::new(NODE_STATE.pdo_sync()),
                });
            }

            Ok(quote! {
                impl #struct_name {
                    pub fn set(&self, idx: usize, value: #field_type) -> Result<(), AbortCode> {
                        if idx >= #array_size {
                            return Err(AbortCode::NoSuchSubIndex)
                        }
                        zencan_node::critical_section::with(|cs| {
                            let mut array = self.array.borrow_ref_mut(cs);
                            array[idx] = value;
                        });
                        Ok(())
                    }

                    pub fn get(&self, idx: usize) -> Result<#field_type, AbortCode> {
                        if idx >= #array_size {
                            return Err(AbortCode::NoSuchSubIndex)
                        }
                        let value = zencan_node::critical_section::with(|cs| {
                            let array = self.array.borrow_ref(cs);
                            array[idx]
                        });
                        Ok(value)
                    }

                    const fn default() -> Self {
                        #struct_name {
                            size: #array_size as u8,
                            array: Mutex::new(RefCell::new([#(#default_tokens),*])),
                            #tpdo_default_tokens
                        }
                    }
                }

                impl ObjectRawAccess for #struct_name {
                    fn write(&self, sub: u8, offset: usize, data: &[u8]) -> Result<(), AbortCode> {
                        if sub == 0 {
                            return Err(AbortCode::ReadOnly);
                        }
                        #write_snippet
                        Ok(())
                    }

                    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<(), AbortCode> {
                        if sub == 0 {
                            if offset != 0 {
                                return Err(AbortCode::UnsupportedAccess);
                            }
                            if buf.len() != 1 {
                                return Err(AbortCode::DataTypeMismatchLengthHigh);
                            }
                            buf[0] = #array_size as u8;
                            return Ok(())
                        }
                        #read_snippet
                        Ok(())
                    }

                    fn sub_info(&self, sub: u8) -> Result<SubInfo, AbortCode> {
                        if sub == 0 {
                            return Ok(SubInfo {
                                access_type: zencan_node::common::objects::AccessType::Ro,
                                data_type: zencan_node::common::objects::DataType::UInt8,
                                size: 1,
                                pdo_mapping: zencan_node::common::objects::PdoMapping::None,
                                persist: false,
                            });
                        }
                        if sub as usize > #array_size {
                            return Err(AbortCode::NoSuchSubIndex);
                        }
                        Ok(SubInfo {
                            access_type: #access_type,
                            data_type: #data_type,
                            size: #storage_size,
                            pdo_mapping: #pdo_mapping,
                            persist: #persist,
                        })
                    }

                    fn object_code(&self) -> zencan_node::common::objects::ObjectCode {
                        zencan_node::common::objects::ObjectCode::Array
                    }

                    #tpdo_event_tokens
                }
            })
        }

        Object::Record(def) => {
            let mut accessor_methods = TokenStream::new();
            let mut write_match_statements = TokenStream::new();
            let mut read_match_statements = TokenStream::new();
            let mut sub_info_match_statements = TokenStream::new();
            let mut default_init_statements = TokenStream::new();

            // For records, sub0 gives the highest sub object support by the record
            let max_sub = def.subs.iter().map(|s| s.sub_index).max().unwrap_or(0);
            let flag_size = (max_sub as usize).div_ceil(8);
            accessor_methods.extend(quote! {
                pub fn get_sub0(&self) -> u8 {
                    #max_sub
                }
            });
            write_match_statements.extend(quote! {
                0 => {
                    Err(AbortCode::ReadOnly)
                }
            });
            let read_snippet = scalar_read_snippet(&format_ident!("sub0"));
            read_match_statements.extend(quote! {
                0 => {
                    #read_snippet
                    Ok(())
                }
            });
            sub_info_match_statements.extend(quote! {
                0 => {
                    Ok(SubInfo {
                        access_type: zencan_node::common::objects::AccessType::Ro,
                        data_type: zencan_node::common::objects::DataType::UInt8,
                        size: 1,
                        pdo_mapping: zencan_node::common::objects::PdoMapping::None,
                        persist: false,
                    })
                }
            });

            for sub in &def.subs {
                let field_name = get_sub_field_name(sub)?;
                let (field_type, size) = get_rust_type_and_size(sub.data_type);
                let read_snippet;
                let write_snippet;
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
                if sub.data_type.is_str() {
                    write_snippet = string_write_snippet(&field_name, size);
                    read_snippet = string_read_snippet(&field_name, size);
                } else {
                    write_snippet = scalar_write_snippet(&field_name, &field_type);
                    read_snippet = scalar_read_snippet(&field_name);
                }
                accessor_methods.extend(quote! {
                    pub fn #setter_name(&self, value: #field_type) {
                        self.#field_name.store(value)
                    }
                    pub fn #getter_name(&self) -> #field_type {
                        self.#field_name.load()
                    }
                });
                write_match_statements.extend(quote! {
                    #sub_index => {
                        #write_snippet
                        Ok(())
                    }
                });
                read_match_statements.extend(quote! {
                    #sub_index => {
                        #read_snippet
                        Ok(())
                    }
                });
                sub_info_match_statements.extend(quote! {
                    #sub_index => {
                        Ok(SubInfo {
                            access_type: #access_type,
                            data_type: #data_type,
                            size: #size,
                            pdo_mapping: #pdo_mapping,
                            persist: #persist,
                        })
                    }
                });
                default_init_statements.extend(quote! {
                    #field_name: AtomicCell::new(#default_tokens),
                });
            }

            let mut tpdo_event_tokens = TokenStream::new();
            let mut tpdo_default_tokens = TokenStream::new();
            if object_supports_tpdo(obj) {
                tpdo_event_tokens.extend(get_tpdo_event_snippet(max_sub as usize));
                tpdo_default_tokens.extend(quote! {
                    flags: ObjectFlags::<#flag_size>::new(NODE_STATE.pdo_sync()),
                })
            }

            Ok(quote! {
                impl #struct_name {
                    #accessor_methods

                    const fn default() -> Self {
                        #struct_name {
                            #default_init_statements
                            #tpdo_default_tokens
                        }
                    }
                }

                impl ObjectRawAccess for #struct_name {
                    fn write(&self, sub: u8, offset: usize, data: &[u8]) -> Result<(), AbortCode> {
                        match sub {
                            #write_match_statements,
                            _ => Err(AbortCode::NoSuchSubIndex),
                        }
                    }

                    fn read(&self, sub: u8, offset: usize, buf: &mut [u8]) -> Result<(), AbortCode> {
                        match sub {
                            #read_match_statements,
                            _ => Err(AbortCode::NoSuchSubIndex),
                        }
                    }

                    fn sub_info(&self, sub: u8) -> Result<SubInfo, AbortCode> {
                        match sub {
                            #sub_info_match_statements
                            _ => Err(AbortCode::NoSuchSubIndex),
                        }
                    }

                    fn object_code(&self) -> zencan_node::common::objects::ObjectCode {
                        zencan_node::common::objects::ObjectCode::Record
                    }
                }
            })
        }
        Object::Domain(_) => todo!(),
    }
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
    quote! {
        pub static NODE_STATE: NodeState<#n_rpdo, #n_tpdo> = NodeState::new();
        pub static NODE_MBOX: NodeMbox = NodeMbox::new(NODE_STATE.rpdos());
    }
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
        if !obj.application_callback {
            object_defs.extend(generate_object_code(obj, &struct_name)?);
            object_instantiations.extend(quote! {
                pub static #inst_name: #struct_name = #struct_name::default();
            });
            table_entries.extend(quote! {
                ODEntry {
                    index: #index,
                    data: ObjectData::Storage(&#inst_name),
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
                    data: ObjectData::Callback(&#inst_name),
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
        use zencan_node::common::objects::{CallbackObject, ObjectFlags, ODEntry, ObjectData, ObjectRawAccess, SubInfo};
        #[allow(unused_imports)]
        use zencan_node::common::sdo::AbortCode;
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
