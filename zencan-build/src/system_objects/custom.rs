//! Generator for custom applicatoin objects
//!
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, Ident};
use zencan_common::object_model::{AccessType, DataType, DefaultValue, PdoMappable};

use crate::errors::CompileError;
use crate::object_build_spec::{
    ObjectBuildShape, ObjectBuildSpec, ObjectImplementation, SubObjectBuildSpec,
    SubObjectImplemention,
};
use crate::system_objects::ObjectInstance;

pub struct CustomObjectGenerator {
    spec: ObjectBuildSpec,
}

impl CustomObjectGenerator {
    /// Create a new custom generated object
    ///
    /// Theses objects are created according to specification provided by the application in the
    /// device config file.
    ///
    /// # Arguments
    /// - `def`: The ObjectBuildSpec defining the object. The implementation must be
    ///   [`Generated`](ObjectImplementation::Generated), or this function
    ///   will panic.
    pub fn new(spec: ObjectBuildSpec) -> Self {
        assert!(matches!(
            spec.implementation,
            ObjectImplementation::Generated
        ));
        Self { spec }
    }

    /// Get the ident to use for the custom struct
    fn type_name(&self) -> TokenStream {
        syn::parse_str(&format!("Object{:X}", self.spec.index)).unwrap()
    }

    /// Get all of the fields of the custom struct
    fn type_fields(&self) -> Result<TokenStream, CompileError> {
        let mut field_tokens = TokenStream::new();
        let mut tpdo_mapping = false;
        let mut highest_sub_index = 0;
        match &self.spec.shape {
            ObjectBuildShape::Record(def) => {
                for (sub_index, sub_spec) in &def.subs {
                    let field_name = get_sub_field_name(*sub_index, sub_spec)?;
                    let field_type = match sub_spec.implementation {
                        SubObjectImplemention::Callback => parse_quote!(CallbackSubObject),
                        SubObjectImplemention::Storage => get_storage_type(sub_spec.spec.data_type),
                    };

                    field_tokens.extend(quote! {
                        pub #field_name: #field_type,
                    });
                    tpdo_mapping |= sub_spec.spec.pdo_mapping.supports_tpdo();
                    highest_sub_index = highest_sub_index.max(*sub_index);
                }
            }
            ObjectBuildShape::Array(def) => {
                let field_type = get_storage_type(def.data_type());
                let array_size = def.len();
                field_tokens.extend(quote! {
                    pub array: [#field_type; #array_size],
                });
                tpdo_mapping |= def.elements().iter().any(|e| e.pdo_mapping.supports_tpdo());
                highest_sub_index = array_size as u8;
            }
            ObjectBuildShape::Var(def) => {
                let field_type = get_storage_type(def.value.data_type);
                field_tokens.extend(quote! {
                    pub value: #field_type,
                });
                tpdo_mapping |= def.value.pdo_mapping.supports_tpdo();
                highest_sub_index = 0;
            }
        }

        if tpdo_mapping {
            let n = (highest_sub_index as usize + 1).div_ceil(8);
            field_tokens.extend(quote! {
                flags: ObjectFlags<#n>,
            });
        }
        Ok(field_tokens)
    }

    fn supports_tpdo(&self) -> bool {
        match &self.spec.shape {
            ObjectBuildShape::Var(v) => v.value.pdo_mapping.supports_tpdo(),
            ObjectBuildShape::Array(a) => {
                a.elements().iter().any(|s| s.pdo_mapping.supports_tpdo())
            }
            ObjectBuildShape::Record(r) => {
                r.subs.values().any(|s| s.spec.pdo_mapping.supports_tpdo())
            }
        }
    }

    fn get_object_code_tokens(&self) -> TokenStream {
        match self.spec.shape {
            ObjectBuildShape::Var(_) => quote!(zencan_node::common::object_model::ObjectCode::Var),
            ObjectBuildShape::Array(_) => {
                quote!(zencan_node::common::object_model::ObjectCode::Array)
            }
            ObjectBuildShape::Record(_) => {
                quote!(zencan_node::common::object_model::ObjectCode::Record)
            }
        }
    }

    fn get_default_init_fn(&self) -> Result<TokenStream, CompileError> {
        match &self.spec.shape {
            ObjectBuildShape::Var(spec) => {
                if matches!(spec.value.data_type, DataType::Domain) {
                    return Ok(quote! {
                        #[inline(never)]
                    fn init_defaults(&self) {}
                    });
                }
                let default_value_tokens = get_default_value_tokens(
                    spec.value.default_value.as_ref(),
                    spec.value.data_type,
                )?;
                Ok(quote! {
                    #[inline(never)]
                    fn init_defaults(&self) {
                        self.value.store(#default_value_tokens);
                    }
                })
            }
            ObjectBuildShape::Array(spec) => {
                if matches!(spec.data_type(), DataType::Domain) {
                    return Ok(quote!(
                        fn init_defaults(&self) {}
                    ));
                }
                // Collect the defaults for all elements into
                let default_values = spec.elements().iter().map(|e| &e.default_value);
                let default_values_tokens: Result<Vec<TokenStream>, _> = default_values
                    .map(|v| get_default_value_tokens(v.as_ref(), spec.data_type()))
                    .collect();
                let default_values_tokens = default_values_tokens?;

                let mut set_statements = TokenStream::new();
                for (i, value) in default_values_tokens.iter().enumerate() {
                    set_statements.extend(quote! {
                        self.array[#i].store(#value);
                    })
                }
                Ok(quote! {
                    #[inline(never)]
                    fn init_defaults(&self) {
                        #set_statements
                    }
                })
            }
            ObjectBuildShape::Record(spec) => {
                let mut store_tokens = TokenStream::new();
                for (sub_index, sub_spec) in spec.subs.iter() {
                    let field_name = get_sub_field_name(*sub_index, sub_spec)?;
                    if sub_spec.implementation == SubObjectImplemention::Callback
                        || matches!(sub_spec.spec.data_type, DataType::Domain)
                    {
                        continue;
                    }
                    let default_value_tokens = get_default_value_tokens(
                        sub_spec.spec.default_value.as_ref(),
                        sub_spec.spec.data_type,
                    )?;
                    store_tokens.extend(quote! {
                        self.#field_name.store(#default_value_tokens);
                    })
                }
                Ok(quote! {
                    #[inline(never)]
                    fn init_defaults(&self) {
                        #store_tokens
                    }
                })
            }
        }
    }

    fn object_access_impl(&self) -> Result<TokenStream, CompileError> {
        let struct_name = self.type_name();
        let mut get_sub_tokens = TokenStream::new();
        let mut accessor_methods = TokenStream::new();

        match &self.spec.shape {
            ObjectBuildShape::Var(var_spec) => {
                let access_type = access_type_to_tokens(var_spec.value.access_type);
                let data_type = data_type_to_tokens(var_spec.value.data_type);
                let field_type = get_rust_type(var_spec.value.data_type);
                let pdo_mapping = pdo_mappable_to_tokens(var_spec.value.pdo_mapping);
                let persist = var_spec.value.persist;

                get_sub_tokens.extend(quote! {
                    match sub {
                        0 => Some((SubInfo {
                            access_type: #access_type,
                            data_type: #data_type,
                            pdo_mapping: #pdo_mapping,
                            persist: #persist,
                        }, &self.value)),
                        _ => None
                    }
                });

                // Domain objects don't get accessors
                if !matches!(var_spec.value.data_type, DataType::Domain) {
                    accessor_methods.extend(quote! {
                        pub fn set_value(&self, value: #field_type) {
                            self.value.store(value);
                        }

                        pub fn get_value(&self) -> #field_type {
                            self.value.load()
                        }
                    });
                }
            }
            ObjectBuildShape::Array(array_spec) => {
                let array_size = array_spec.len();
                let field_type = get_rust_type(array_spec.data_type());

                get_sub_tokens.extend(quote! {
                    if sub == 0 {
                        Some((
                            SubInfo::MAX_SUB_NUMBER,
                            const { &ConstField::new((#array_size as u8).to_le_bytes()) },
                        ))
                    }
                });
                for (i, sub) in array_spec.elements().iter().enumerate() {
                    let sub_index = i as u8 + 1;
                    let access_type = access_type_to_tokens(sub.access_type);
                    let data_type = data_type_to_tokens(sub.data_type);
                    let pdo_mapping = pdo_mappable_to_tokens(sub.pdo_mapping);
                    let persist = sub.persist;
                    get_sub_tokens.extend(quote! {
                        else if sub == #sub_index {
                            Some((SubInfo {
                                access_type: #access_type,
                                data_type: #data_type,
                                pdo_mapping: #pdo_mapping,
                                persist: #persist,
                            }, &self.array[#i]))
                        }
                    });
                }
                get_sub_tokens.extend(quote! {
                    else {
                        None
                    }
                });

                if !matches!(array_spec.data_type(), DataType::Domain) {
                    accessor_methods.extend(quote! {
                        pub fn set(
                            &self,
                            index: usize,
                            value: #field_type
                        ) -> Result<(), AbortCode> {
                            if index < #array_size {
                                self.array[index].store(value);
                                Ok(())
                            } else {
                                Err(AbortCode::NoSuchSubIndex)
                            }
                        }

                        pub fn get(&self, index: usize) -> Result<#field_type, AbortCode> {
                            if index < #array_size {
                                Ok(self.array[index].load())
                            } else {
                                Err(AbortCode::NoSuchSubIndex)
                            }
                        }
                    });
                }
            }
            ObjectBuildShape::Record(record_build_spec) => {
                let mut match_statements = TokenStream::new();

                // For records, sub0 gives the highest sub object support by the record
                let max_sub = record_build_spec.subs.keys().max().unwrap_or(&0);

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

                for (sub_index, sub) in &record_build_spec.subs {
                    let field_name = format_ident!(
                        "{}",
                        sub.field_name
                            .clone()
                            .unwrap_or(format!("sub{}", sub_index))
                    );
                    let field_type = get_rust_type(sub.spec.data_type);
                    let setter_name = format_ident!("set_{}", field_name);
                    let getter_name = format_ident!("get_{}", field_name);
                    let data_type = data_type_to_tokens(sub.spec.data_type);
                    let access_type = access_type_to_tokens(sub.spec.access_type);
                    let pdo_mapping = pdo_mappable_to_tokens(sub.spec.pdo_mapping);
                    let persist = sub.spec.persist;

                    // let default_value = sub
                    //     .default_value
                    //     .clone()
                    //     .or_else(|| default_default_value(sub.data_type));
                    // let default_tokens = get_default_tokens(default_value.as_ref(), sub.data_type)?;

                    // Domains don't get accessors
                    if sub.implementation == SubObjectImplemention::Storage
                        && !matches!(sub.spec.data_type, DataType::Domain)
                    {
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
                                    pdo_mapping: #pdo_mapping,
                                    persist: #persist,
                                },
                                &self.#field_name
                            )
                        ),
                    });
                    // default_init_tokens.extend(quote! {
                    //     #field_name: #default_tokens,
                    // });
                }

                get_sub_tokens.extend(quote! {
                    match sub {
                        #match_statements
                        _ => None,
                    }
                });
            }
        };

        let object_code = self.get_object_code_tokens();
        let default_init_fn = self.get_default_init_fn()?;
        let reset_fn = if self.supports_tpdo() {
            quote! {
                fn reset_object(&self) { self.init_defaults(); self.flags.reset(); }
            }
        } else {
            quote! {fn reset_object(&self) {self.init_defaults(); } }
        };
        let flags = if self.supports_tpdo() {
            quote!(
                fn flags(&self) -> Option<&dyn ObjectFlagAccess> {
                    Some(&self.flags)
                }
            )
        } else {
            quote!()
        };
        Ok(quote! {
            impl #struct_name {
                #accessor_methods

                #default_init_fn
                #reset_fn
            }

            impl ProvidesSubObjects for #struct_name {
                #flags
                fn get_sub_object(&self, sub: u8) -> Option<(SubInfo, &dyn SubObjectAccess)> {
                    #get_sub_tokens
                }

                fn object_code(&self) -> zencan_node::common::object_model::ObjectCode {
                    #object_code
                }
            }
        })
    }
}

impl super::ObjectGenerator for CustomObjectGenerator {
    fn type_definitions(&self) -> Result<TokenStream, CompileError> {
        let struct_name = self.type_name();
        let fields = self.type_fields()?;
        let impls = self.object_access_impl()?;
        Ok(quote! {
            #[allow(dead_code)]
            pub struct #struct_name {
                #fields
            }

            #impls

        })
    }

    fn instance(&self) -> Result<Option<ObjectInstance>, CompileError> {
        let field_name: Ident =
            syn::parse_str(&self.spec.object_name).map_err(|_| CompileError::InvalidFieldName {
                field_name: self.spec.object_name.clone(),
            })?;
        let object_type = self.type_name();
        let mut fields = TokenStream::new();
        match &self.spec.shape {
            ObjectBuildShape::Var(_) => fields.extend(quote!(value: Default::default(),)),
            ObjectBuildShape::Array(_) => {
                fields.extend(quote!(array: core::array::from_fn(|_| Default::default()),))
            }
            ObjectBuildShape::Record(record) => {
                for (index, sub) in &record.subs {
                    let name = get_sub_field_name(*index, sub)?;
                    fields.extend(quote!(#name: Default::default(),));
                }
            }
        }
        if self.supports_tpdo() {
            fields.extend(quote!(flags: ObjectFlags::new(node_state.object_flag_sync()),));
        }
        Ok(Some(ObjectInstance {
            field_name,
            object_type: object_type.clone(),
            initializer: quote!(#object_type { #fields }),
            reset: Some(quote!(reset_object())),
        }))
    }
}

fn get_sub_field_name(sub_index: u8, sub: &SubObjectBuildSpec) -> Result<syn::Ident, CompileError> {
    match &sub.field_name {
        Some(field_name) => {
            // Validate that the given field name is a valid rust identifier
            match syn::parse_str::<syn::Ident>(field_name) {
                Ok(ident) => Ok(ident),
                Err(_) => Err(CompileError::InvalidFieldName {
                    field_name: field_name.to_string(),
                }),
            }
        }
        None => {
            // Unwrap safety: This should always yield a valid identifier
            Ok(syn::parse_str(&format!("sub{}", sub_index)).unwrap())
        }
    }
}

/// Get the struct attribute type used to store this type
fn get_storage_type(data_type: DataType) -> syn::Type {
    match data_type {
        DataType::Boolean => syn::parse_quote!(ScalarFieldBool),
        DataType::Int8 => syn::parse_quote!(ScalarFieldI8),
        DataType::Int16 => syn::parse_quote!(ScalarFieldI16),
        DataType::Int24 => syn::parse_quote!(ScalarFieldI24),
        DataType::Int32 => syn::parse_quote!(ScalarFieldI32),
        DataType::Int64 => syn::parse_quote!(ScalarFieldI64),
        DataType::UInt8 => syn::parse_quote!(ScalarFieldU8),
        DataType::UInt16 => syn::parse_quote!(ScalarFieldU16),
        DataType::UInt24 => syn::parse_quote!(ScalarFieldU24),
        DataType::UInt32 => syn::parse_quote!(ScalarFieldU32),
        DataType::UInt64 => syn::parse_quote!(ScalarFieldU64),
        DataType::Real32 => syn::parse_quote!(ScalarFieldF32),
        DataType::Real64 => syn::parse_quote!(ScalarFieldF64),
        DataType::VisibleString(n) | DataType::UnicodeString(n) => {
            syn::parse_str(&format!("NullTermByteField::<{}>", n)).unwrap()
        }
        DataType::OctetString(n) => syn::parse_str(&format!("ByteField::<{}>", n)).unwrap(),
        DataType::TimeOfDay => syn::parse_quote!(ScalarField<TimeOfDay>),
        DataType::TimeDifference => syn::parse_quote!(ScalarField<TimeDifference>),
        DataType::Domain => syn::parse_quote!(CallbackSubObject),
    }
}

/// Convert an AccessType enum to a tokenstream expressing the variant
fn access_type_to_tokens(at: AccessType) -> TokenStream {
    match at {
        AccessType::Ro => quote!(zencan_node::common::object_model::AccessType::Ro),
        AccessType::Wo => quote!(zencan_node::common::object_model::AccessType::Wo),
        AccessType::Rw => quote!(zencan_node::common::object_model::AccessType::Rw),
        AccessType::Const => quote!(zencan_node::common::object_model::AccessType::Const),
    }
}

fn data_type_to_tokens(dt: DataType) -> TokenStream {
    match dt {
        DataType::Boolean => quote!(zencan_node::common::object_model::DataType::Boolean),
        DataType::Int8 => quote!(zencan_node::common::object_model::DataType::Int8),
        DataType::Int16 => quote!(zencan_node::common::object_model::DataType::Int16),
        DataType::Int24 => quote!(zencan_node::common::object_model::DataType::Int24),
        DataType::Int32 => quote!(zencan_node::common::object_model::DataType::Int32),
        DataType::Int64 => quote!(zencan_node::common::object_model::DataType::Int64),
        DataType::UInt8 => quote!(zencan_node::common::object_model::DataType::UInt8),
        DataType::UInt16 => quote!(zencan_node::common::object_model::DataType::UInt16),
        DataType::UInt24 => quote!(zencan_node::common::object_model::DataType::UInt24),
        DataType::UInt32 => quote!(zencan_node::common::object_model::DataType::UInt32),
        DataType::UInt64 => quote!(zencan_node::common::object_model::DataType::UInt64),
        DataType::Real32 => quote!(zencan_node::common::object_model::DataType::Real32),
        DataType::Real64 => quote!(zencan_node::common::object_model::DataType::Real64),
        DataType::VisibleString(n) => {
            quote!(zencan_node::common::object_model::DataType::VisibleString(
                #n
            ))
        }
        DataType::UnicodeString(n) => {
            quote!(zencan_node::common::object_model::DataType::UnicodeString(
                #n
            ))
        }
        DataType::OctetString(n) => {
            quote!(zencan_node::common::object_model::DataType::OctetString(#n))
        }
        DataType::TimeOfDay => quote!(zencan_node::common::object_model::DataType::TimeOfDay),
        DataType::TimeDifference => {
            quote!(zencan_node::common::object_model::DataType::TimeDifference)
        }
        DataType::Domain => quote!(zencan_node::common::object_model::DataType::Domain),
    }
}

fn pdo_mappable_to_tokens(p: PdoMappable) -> TokenStream {
    match p {
        PdoMappable::None => quote!(zencan_node::common::object_model::PdoMappable::None),
        PdoMappable::Tpdo => quote!(zencan_node::common::object_model::PdoMappable::Tpdo),
        PdoMappable::Rpdo => quote!(zencan_node::common::object_model::PdoMappable::Rpdo),
        PdoMappable::Both => quote!(zencan_node::common::object_model::PdoMappable::Both),
    }
}

fn get_rust_type(data_type: DataType) -> syn::Type {
    match data_type {
        DataType::Boolean => syn::parse_quote!(bool),
        DataType::Int8 => syn::parse_quote!(i8),
        DataType::Int16 => syn::parse_quote!(i16),
        DataType::Int24 => syn::parse_quote!(i24),
        DataType::Int32 => syn::parse_quote!(i32),
        DataType::Int64 => syn::parse_quote!(i64),
        DataType::UInt8 => syn::parse_quote!(u8),
        DataType::UInt16 => syn::parse_quote!(u16),
        DataType::UInt24 => syn::parse_quote!(u24),
        DataType::UInt32 => syn::parse_quote!(u32),
        DataType::UInt64 => syn::parse_quote!(u64),
        DataType::Real32 => syn::parse_quote!(f32),
        DataType::Real64 => syn::parse_quote!(f64),
        DataType::VisibleString(n) | DataType::OctetString(n) | DataType::UnicodeString(n) => {
            syn::parse_str(&format!("[u8; {}]", n)).unwrap()
        }
        DataType::TimeOfDay => syn::parse_quote!(TimeOfDay),
        DataType::TimeDifference => syn::parse_quote!(TimeDifference),
        DataType::Domain => syn::parse_quote!(None),
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

/// Get DefaultValue for a given data type. This is the default value when none is provided.
fn default_default(data_type: DataType) -> Option<DefaultValue> {
    match data_type {
        DataType::Boolean
        | DataType::Int8
        | DataType::Int16
        | DataType::Int24
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt24
        | DataType::UInt32 => Some(DefaultValue::Integer(0)),
        DataType::UInt64 => Some(DefaultValue::Integer(0)),
        DataType::Real32 | DataType::Real64 => Some(DefaultValue::Float(0.0)),
        DataType::VisibleString(_) | DataType::UnicodeString(_) | DataType::OctetString(_) => {
            Some(DefaultValue::String("".to_string()))
        }
        _ => None,
    }
}

fn get_default_value_tokens(
    value: Option<&DefaultValue>,
    data_type: DataType,
) -> Result<TokenStream, CompileError> {
    if matches!(data_type, DataType::Domain) {
        panic!("No default for callback");
    }
    let default_value = default_default(data_type);
    let value = value.or(default_value.as_ref());
    if value.is_none() {
        return Ok(quote!(Default::default()));
    }
    match value.unwrap() {
        DefaultValue::String(s) => {
            if !data_type.is_str() {
                return Err(CompileError::DefaultValueTypeMismatch {
                    message: format!(
                        "Default string value '{}' is not a string for type {:?}",
                        s, data_type
                    ),
                });
            }
            string_to_byte_literal_tokens(s, data_type.size())
        }
        DefaultValue::Float(f) => match data_type {
            DataType::Real32 => Ok(quote!(#f as f32)),
            DataType::Real64 => Ok(quote!(#f)),
            _ => Err(CompileError::DefaultValueTypeMismatch {
                message: format!(
                    "Default float value {} is not a valid value for type {:?}",
                    f, data_type
                ),
            }),
        },
        DefaultValue::Integer(i) => {
            // Create token as stream so the literal does not have an explicit type (e.g. '32' instead of '32i64')
            match data_type {
                DataType::Boolean => {
                    if *i != 0 {
                        Ok(quote!(true))
                    } else {
                        Ok(quote!(false))
                    }
                }
                DataType::Int8 => Ok(quote!(#i as i8)),
                DataType::Int16 => Ok(quote!(#i as i16)),
                DataType::Int24 => Ok(quote!(i24::new(#i as i32))),
                DataType::Int32 => Ok(quote!(#i as i32)),
                DataType::Int64 => Ok(quote!(#i)),
                DataType::UInt8 => Ok(quote!(#i as u8)),
                DataType::UInt16 => Ok(quote!(#i as u16)),
                DataType::UInt24 => Ok(quote!(u24::new(#i as u32))),
                DataType::UInt32 => Ok(quote!(#i as u32)),
                DataType::UInt64 => Ok(quote!(#i as u64)),
                DataType::Real32 => Ok(quote!(#i as f32)),
                DataType::Real64 => Ok(quote!(#i as f64)),
                _ => Err(CompileError::DefaultValueTypeMismatch {
                    message: format!(
                        "Default integer value {} is not a valid value for type {:?}",
                        i, data_type
                    ),
                }),
            }
        }
    }
}
