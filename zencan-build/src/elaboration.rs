//! Elaboration creates the object dictionary build specification from a device config

use std::collections::{BTreeMap, HashMap};

use zencan_common::object_model::{
    AccessType, ArraySpec, DataType, DefaultValue, PdoMappable, SubObjectSpec, VarSpec,
};

use crate::{
    device_config::{AutoStartConfig, BootloaderConfig, DeviceConfig, ObjectDefinition},
    errors::CompileError,
    object_build_spec::{
        ObjectBuildShape, ObjectBuildSpec, ObjectImplementation, RecordBuildSpec,
        SubObjectBuildSpec, SubObjectImplemention, SystemObjectKind,
    },
};

fn mandatory_objects(config: &DeviceConfig) -> Vec<ObjectBuildSpec> {
    let mut objects = vec![
        ObjectBuildSpec {
            index: 0x1000,
            parameter_name: "Device Type".to_string(),
            object_name: "object1000".to_string(),
            shape: ObjectBuildShape::Var(VarSpec {
                value: SubObjectSpec {
                    parameter_name: "Device Type".to_string(),
                    data_type: DataType::UInt32,
                    access_type: AccessType::Const,
                    default_value: Some(DefaultValue::Integer(0x00000000)),
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
            }),
            implementation: ObjectImplementation::Generated,
        },
        ObjectBuildSpec {
            index: 0x1001,
            parameter_name: "Error Register".to_string(),
            object_name: "object1001".to_string(),
            shape: ObjectBuildShape::Var(VarSpec {
                value: SubObjectSpec {
                    parameter_name: "Error Register".to_string(),
                    data_type: DataType::UInt8,
                    access_type: AccessType::Ro,
                    default_value: Some(DefaultValue::Integer(0x00000000)),
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
            }),
            implementation: ObjectImplementation::Generated,
        },
        ObjectBuildSpec {
            index: 0x1008,
            parameter_name: "Manufacturer Device Name".to_string(),
            object_name: "object1008".to_string(),
            shape: ObjectBuildShape::Var(VarSpec {
                value: SubObjectSpec {
                    parameter_name: "Manufacturer Device Name".to_string(),
                    data_type: DataType::VisibleString(config.device_name.len()),
                    access_type: AccessType::Const,
                    default_value: Some(DefaultValue::String(config.device_name.clone())),
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
            }),
            implementation: ObjectImplementation::Generated,
        },
        ObjectBuildSpec {
            index: 0x1009,
            parameter_name: "Manufacturer Hardware Version".to_string(),
            object_name: "object1009".to_string(),
            shape: ObjectBuildShape::Var(VarSpec {
                value: SubObjectSpec {
                    parameter_name: "Manufacturer Hardware Version".to_string(),
                    data_type: DataType::VisibleString(config.hardware_version.len()),
                    access_type: AccessType::Const,
                    default_value: Some(DefaultValue::String(config.hardware_version.clone())),
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
            }),
            implementation: ObjectImplementation::Generated,
        },
        ObjectBuildSpec {
            index: 0x100A,
            parameter_name: "Manufacturer Software Version".to_string(),
            object_name: "object100A".to_string(),
            shape: ObjectBuildShape::Var(VarSpec {
                value: SubObjectSpec {
                    parameter_name: "Manufacturer Software Version".to_string(),
                    data_type: DataType::VisibleString(config.software_version.len()),
                    access_type: AccessType::Const,
                    default_value: Some(DefaultValue::String(config.software_version.clone())),
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
            }),
            implementation: ObjectImplementation::Generated,
        },
        ObjectBuildSpec {
            index: 0x1017,
            parameter_name: "Heartbeat Producer Time (ms)".to_string(),
            object_name: "object1017".to_string(),
            shape: ObjectBuildShape::Var(VarSpec {
                value: SubObjectSpec {
                    parameter_name: "Heartbeat Producer Time (ms)".to_string(),
                    data_type: DataType::UInt16,
                    access_type: AccessType::Const,
                    default_value: Some(DefaultValue::Integer(config.heartbeat_period as i64)),
                    pdo_mapping: PdoMappable::None,
                    persist: false,
                },
            }),
            implementation: ObjectImplementation::Generated,
        },
        ObjectBuildSpec {
            index: 0x1018,
            parameter_name: "Identity".to_string(),
            object_name: "object1018".to_string(),
            shape: ObjectBuildShape::Record(RecordBuildSpec {
                subs: BTreeMap::from([
                    (
                        1,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Vendor ID".to_string(),
                                data_type: DataType::UInt32,
                                access_type: AccessType::Const,
                                default_value: Some(DefaultValue::Integer(
                                    config.identity.vendor_id as i64,
                                )),
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Storage,
                            field_name: Some("vendor_id".to_string()),
                        },
                    ),
                    (
                        2,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Product Code".to_string(),
                                data_type: DataType::UInt32,
                                access_type: AccessType::Const,
                                default_value: Some(DefaultValue::Integer(
                                    config.identity.product_code as i64,
                                )),
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Storage,
                            field_name: Some("product_code".to_string()),
                        },
                    ),
                    (
                        3,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Revision Number".to_string(),
                                data_type: DataType::UInt32,
                                access_type: AccessType::Const,
                                default_value: Some(DefaultValue::Integer(
                                    config.identity.revision_number as i64,
                                )),
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Storage,
                            field_name: Some("revision".to_string()),
                        },
                    ),
                    (
                        4,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Serial Number".to_string(),
                                data_type: DataType::UInt32,
                                access_type: AccessType::Const,
                                default_value: Some(DefaultValue::Integer(0)),
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Storage,
                            field_name: Some("serial".to_string()),
                        },
                    ),
                ]),
            }),
            implementation: ObjectImplementation::System(SystemObjectKind::Identity {
                vendor: config.identity.vendor_id,
                product: config.identity.product_code,
                revision: config.identity.revision_number,
            }),
        },
    ];

    let (create_autostart, default) = match config.autostart {
        AutoStartConfig::Disabled => (true, 0),
        AutoStartConfig::Enabled => (true, 1),
        AutoStartConfig::Unsupported => (false, 0),
    };
    if create_autostart {
        objects.push(ObjectBuildSpec {
            index: 0x5000,
            parameter_name: "Auto Start".to_string(),
            object_name: "object5000".to_string(),
            shape: ObjectBuildShape::Var(VarSpec {
                value: SubObjectSpec {
                    parameter_name: "Auto Start".to_string(),
                    data_type: DataType::UInt8,
                    access_type: AccessType::Rw,
                    default_value: Some(DefaultValue::Integer(default)),
                    pdo_mapping: PdoMappable::None,
                    persist: true,
                },
            }),
            implementation: ObjectImplementation::Generated,
        });
    }

    objects
}

/// Create PDO related object specifications
fn pdo_objects(num_rpdo: usize, num_tpdo: usize) -> Vec<ObjectBuildSpec> {
    let mut objects = Vec::new();

    fn add_objects(objects: &mut Vec<ObjectBuildSpec>, i: usize, tx: bool) {
        let pdo_type = if tx { "TPDO" } else { "RPDO" };
        let comm_index = if tx { 0x1800 } else { 0x1400 };
        let mapping_index = if tx { 0x1A00 } else { 0x1600 };

        objects.push(ObjectBuildSpec {
            index: comm_index + i as u16,
            parameter_name: format!("{}{} Communication Parameter", pdo_type, i),
            object_name: format!("{}{}_comm", pdo_type.to_lowercase(), i),
            implementation: ObjectImplementation::System(SystemObjectKind::Pdo),

            shape: ObjectBuildShape::Record(RecordBuildSpec {
                subs: BTreeMap::from_iter([
                    (
                        1,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: format!("COB-ID for {}{}", pdo_type, i),
                                data_type: DataType::UInt32,
                                access_type: AccessType::Rw,
                                default_value: None,
                                pdo_mapping: PdoMappable::None,
                                persist: true,
                            },
                            field_name: Some("cob_id".to_string()),
                            implementation: SubObjectImplemention::Storage,
                        },
                    ),
                    (
                        2,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: format!("Transmission type for {}{}", pdo_type, i),
                                data_type: DataType::UInt8,
                                access_type: AccessType::Rw,
                                default_value: None,
                                pdo_mapping: PdoMappable::None,
                                persist: true,
                            },
                            field_name: Some("transmission_type".to_string()),
                            implementation: SubObjectImplemention::Storage,
                        },
                    ),
                ]),
            }),
        });

        let mut mapping_subs = BTreeMap::new();

        mapping_subs.insert(
            0,
            SubObjectBuildSpec {
                spec: SubObjectSpec {
                    parameter_name: "Valid Mappings".to_string(),
                    data_type: DataType::UInt8,
                    access_type: AccessType::Rw,
                    default_value: Some(DefaultValue::Integer(0)),
                    pdo_mapping: PdoMappable::None,
                    persist: true,
                },
                field_name: None,
                implementation: SubObjectImplemention::Storage,
            },
        );

        for sub in 1..65 {
            mapping_subs.insert(
                sub,
                SubObjectBuildSpec {
                    spec: SubObjectSpec {
                        parameter_name: format!("{}{} Mapping App Object {}", pdo_type, i, sub),
                        data_type: DataType::UInt32,
                        access_type: AccessType::Rw,
                        default_value: None,
                        pdo_mapping: PdoMappable::None,
                        persist: true,
                    },
                    field_name: None,
                    implementation: SubObjectImplemention::Storage,
                },
            );
        }

        objects.push(ObjectBuildSpec {
            index: mapping_index + i as u16,
            object_name: format!("{}{}_mapping", pdo_type.to_lowercase(), i),
            parameter_name: format!("{}{} Mapping Parameters", pdo_type, i),
            implementation: ObjectImplementation::System(SystemObjectKind::Pdo),
            shape: ObjectBuildShape::Record(RecordBuildSpec { subs: mapping_subs }),
        });
    }
    for i in 0..num_rpdo {
        add_objects(&mut objects, i, false);
    }
    for i in 0..num_tpdo {
        add_objects(&mut objects, i, true);
    }
    objects
}

fn bootloader_objects(cfg: &BootloaderConfig) -> Vec<ObjectBuildSpec> {
    let mut objects = Vec::new();

    if cfg.sections.is_empty() {
        return objects;
    }
    objects.push(ObjectBuildSpec {
        index: 0x5500,
        parameter_name: "Bootloader Info".into(),
        object_name: "object5500".to_string(),
        implementation: ObjectImplementation::System(SystemObjectKind::BootloaderInfo {
            application: cfg.application,
            num_sections: cfg.sections.len(),
        }),
        shape: ObjectBuildShape::Record(RecordBuildSpec {
            subs: BTreeMap::from([
                (
                    1,
                    SubObjectBuildSpec {
                        spec: SubObjectSpec {
                            parameter_name: "Bootloader Config".into(),
                            data_type: DataType::UInt32,
                            access_type: AccessType::Ro,
                            default_value: Some(0.into()),
                            pdo_mapping: PdoMappable::None,
                            persist: false,
                        },
                        implementation: SubObjectImplemention::Storage,
                        field_name: Some("config".into()),
                    },
                ),
                (
                    2,
                    SubObjectBuildSpec {
                        spec: SubObjectSpec {
                            parameter_name: "Number of Section".into(),
                            data_type: DataType::UInt8,
                            access_type: AccessType::Ro,
                            default_value: Some(cfg.sections.len().into()),
                            pdo_mapping: PdoMappable::None,
                            persist: false,
                        },
                        implementation: SubObjectImplemention::Storage,
                        field_name: Some("num_sections".into()),
                    },
                ),
                (
                    3,
                    SubObjectBuildSpec {
                        spec: SubObjectSpec {
                            parameter_name: "Reset to Bootloader Command".into(),
                            data_type: DataType::UInt32,
                            access_type: AccessType::Wo,
                            default_value: None,
                            pdo_mapping: PdoMappable::None,
                            persist: false,
                        },
                        implementation: SubObjectImplemention::Storage,
                        field_name: None,
                    },
                ),
            ]),
        }),
    });

    for (i, section) in cfg.sections.iter().enumerate() {
        objects.push(ObjectBuildSpec {
            index: 0x5510 + i as u16,
            parameter_name: format!("Bootloader Section {i}"),
            object_name: format!("object{:x}", 0x5510 + i as u16),
            implementation: ObjectImplementation::System(SystemObjectKind::BootloaderSection {
                name: section.name.clone(),
                size: section.size,
            }),
            shape: ObjectBuildShape::Record(RecordBuildSpec {
                subs: BTreeMap::from([
                    (
                        1,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Mode bits".into(),
                                data_type: DataType::UInt8,
                                access_type: AccessType::Const,
                                default_value: None,
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Callback,
                            field_name: None,
                        },
                    ),
                    (
                        2,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Section Name".into(),
                                data_type: DataType::VisibleString(0),
                                access_type: AccessType::Const,
                                default_value: Some(section.name.as_str().into()),
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Callback,
                            field_name: None,
                        },
                    ),
                    (
                        3,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Section Size".into(),
                                data_type: DataType::UInt32,
                                access_type: AccessType::Const,
                                default_value: Some((section.size as i64).into()),
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Callback,
                            field_name: None,
                        },
                    ),
                    (
                        4,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Erase Command".into(),
                                data_type: DataType::UInt8,
                                access_type: AccessType::Wo,
                                default_value: None,
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Callback,
                            field_name: None,
                        },
                    ),
                    (
                        5,
                        SubObjectBuildSpec {
                            spec: SubObjectSpec {
                                parameter_name: "Data".into(),
                                data_type: DataType::Domain,
                                access_type: AccessType::Rw,
                                default_value: None,
                                pdo_mapping: PdoMappable::None,
                                persist: false,
                            },
                            implementation: SubObjectImplemention::Callback,
                            field_name: None,
                        },
                    ),
                ]),
            }),
        });
    }

    objects
}

fn object_storage_objects() -> Vec<ObjectBuildSpec> {
    vec![ObjectBuildSpec {
        index: 0x1010,
        parameter_name: "Object Save Command".to_string(),
        object_name: "object1010".to_string(),
        implementation: ObjectImplementation::System(SystemObjectKind::StorageCommand),
        shape: ObjectBuildShape::Array(ArraySpec::new(vec![SubObjectSpec {
            parameter_name: "Object Save Command".to_string(),
            data_type: DataType::UInt32,
            access_type: AccessType::Rw,
            default_value: None,
            pdo_mapping: PdoMappable::None,
            persist: false,
        }])),
    }]
}

fn elaborate_generated_object(def: &ObjectDefinition) -> ObjectBuildSpec {
    let shape = match &def.object {
        crate::device_config::Object::Var(var_definition) => ObjectBuildShape::Var(VarSpec {
            value: SubObjectSpec {
                parameter_name: def.parameter_name.clone(),
                data_type: var_definition.data_type,
                access_type: var_definition.access_type,
                default_value: var_definition.default_value.clone(),
                pdo_mapping: var_definition.pdo_mapping,
                persist: var_definition.persist,
            },
        }),
        crate::device_config::Object::Array(array_definition) => {
            let mut elements = Vec::new();
            for i in 0..array_definition.array_size {
                let default_value = array_definition.default_value.as_ref().map(|defaults| {
                    defaults
                        .get(i)
                        .unwrap_or_else(|| {
                            panic!(
                                "Not enough default value for array in object 0x{:x}",
                                def.index
                            )
                        })
                        .clone()
                });
                elements.push(SubObjectSpec {
                    parameter_name: format!("{} {}", def.parameter_name, i),
                    data_type: array_definition.data_type,
                    access_type: array_definition.access_type,
                    default_value,
                    pdo_mapping: array_definition.pdo_mapping,
                    persist: array_definition.persist,
                })
            }
            ObjectBuildShape::Array(ArraySpec::new(elements))
        }
        crate::device_config::Object::Record(record_definition) => {
            let mut subs = BTreeMap::new();
            for sub in &record_definition.subs {
                subs.insert(
                    sub.sub_index,
                    SubObjectBuildSpec {
                        spec: SubObjectSpec {
                            parameter_name: sub.parameter_name.clone(),
                            data_type: sub.data_type,
                            access_type: sub.access_type,
                            default_value: sub.default_value.clone(),
                            pdo_mapping: sub.pdo_mapping,
                            persist: sub.persist,
                        },
                        // Note: Future device config implementation will allow
                        // for sub-object calbacks. For now, they are always
                        // generated as storage
                        implementation: SubObjectImplemention::Storage,
                        field_name: sub.field_name.clone(),
                    },
                );
            }
            ObjectBuildShape::Record(RecordBuildSpec { subs })
        }
    };
    let implementation = if def.application_callback {
        ObjectImplementation::Callback
    } else {
        ObjectImplementation::Generated
    };
    ObjectBuildSpec {
        index: def.index,
        parameter_name: def.parameter_name.clone(),
        object_name: format!("object{:04x}", def.index),
        shape,
        implementation,
    }
}

impl DeviceConfig {
    /// Create the specs for an object dictionary from a DeviceConfig
    pub fn elaborate(&self) -> Result<Vec<ObjectBuildSpec>, CompileError> {
        let mut objects = Vec::new();
        // Add the mandatory objects that all systems contain
        objects.extend(mandatory_objects(self));
        // Add PDO objects
        objects.extend(pdo_objects(
            self.pdos.num_rpdo as usize,
            self.pdos.num_tpdo as usize,
        ));
        // Add user-defined objects
        for custom_obj in &self.objects {
            objects.push(elaborate_generated_object(custom_obj));
        }
        // Add bootloader objects
        objects.extend(bootloader_objects(&self.bootloader));
        // Add object storage command object
        if self.support_storage {
            objects.extend(object_storage_objects());
        }

        // Check ID uniqueness
        let mut ids: HashMap<u16, &ObjectBuildSpec> = HashMap::new();
        for obj in &objects {
            if ids.contains_key(&obj.index) {
                return Err(CompileError::DuplicateObjectIndex {
                    object1: Box::new(ids[&obj.index].into()),
                    object2: Box::new(obj.into()),
                });
            }
            ids.insert(obj.index, obj);
        }
        Ok(objects)
    }
}
