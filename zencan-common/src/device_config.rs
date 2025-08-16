//! Device config file
//!
//! A DeviceConfig is created from a TOML file, and provides build-time configuration for a zencan
//! node. The device config specifies all of the objects in the object dictionary of the node,
//! including custom ones defined for the specific application.
//!
//! # An example TOML file
//!
//! ```toml
//! device_name = "can-io"
//! software_version = "v0.0.1"
//! hardware_version = "rev1"
//! heartbeat_period = 1000
//!
//! # Define 3 out of 4 device unique identifiers. These define the application/device, the fourth is
//! # the serial number, which must be provided at run-time by the application.
//! [identity]
//! vendor_id = 0xCAFE
//! product_code = 1032
//! revision_number = 1
//!
//! # Defines the number of PDOs the device will support
//! [pdos]
//! num_rpdo = 4
//! num_tpdo = 4
//!
//! # User's can create custom objects to hold application specific data
//! [[objects]]
//! index = 0x2000
//! parameter_name = "Raw Analog Input"
//! object_type = "array"
//! data_type = "uint16"
//! access_type = "ro"
//! array_size = 4
//! default_value = [0, 0, 0, 0]
//! pdo_mapping = "tpdo"
//! ```
//!
//! # Object Namespaces
//!
//! Application specific objects should be defined in the range 0x2000-0x4fff. Many objects will be
//! created by default in addition to the ones defined by the user.
//!
//! # Standard Objects
//!
//! ## 0x1008 - Device Name
//!
//! A VAR object containing a string with a human readable device name. This value is set by
//! [DeviceConfig::device_name].
//!
//! ## 0x1009 - Hardware Version
//!
//! A VAR object containing a string with a human readable hardware version. This value is set by
//! [DeviceConfig::hardware_version].
//!
//! ## 0x100A - Software Version
//!
//! A VAR object containing a string with a human readable software version. This value is set by
//! [DeviceConfig::software_version]
//!
//! ## 0x1010 - Object Save Command
//!
//! An array object used to command the node to store its current object values.
//!
//! Array size: 1 Data type: u32
//!
//! When read, sub-object 1 will return a 1 if a storage callback has been provided by the
//! application, indicating that saving is supported.
//!
//! To trigger a save, write a u32 with the [magic value](crate::constants::values::SAVE_CMD).
//!
//! ## 0x1017 - Heartbeat Producer Time
//!
//! A VAR object of type U16.
//!
//! This object stores the period at which the heartbeat is sent by the device, in milliseconds. It
//! is set by [DeviceConfig::heartbeat_period].
//!
//! ## 0x1018 - Identity
//!
//! A record object which stores the 128-bit unique identifier for the node.
//!
//! | Sub Object | Type | Description |
//! | ---------- | ---- | ----------- |
//! | 0          | u8   | Max sub index - always 4 |
//! | 1          | u32  | Vendor ID    |
//! | 2          | u32  | Product Code |
//! | 3          | u32  | Revision |
//! | 4          | u32  | Serial |
//!
//! ## 0x1400 to 0x1400 + N - RPDO Communications Parameter
//!
//! One object for each RPDO supported by the node. This configures how the PDO is received.
//!
//! ## 0x1600 to 0x1600 + N - RPDO Mapping Parameters
//!
//! One object for each RPDO supported by the node. This configures which sub objects the data in
//! the PDO message maps to.
//!
//! Sub Object 0 contains the number of valid mappings. Sub objects 1 through 9 specify a list of
//! sub objects to map to.
//!
//! ## 0x1800 to 0x1800 + N - TPDO Communications Parameter
//!
//! One object for each TPDO supported by the node. This configures how the PDO is transmitted.
//!
//! ## 0x1A00 to 0x1A00 + N - TPDO Mapping Parameters
//!
//! One object for each TPDO supported by the node. This configures which sub objects the data in
//! the PDO message maps to.
//!
//! Sub Object 0 contains the number of valid mappings. Sub objects 1 through 9 specify a list of
//! sub objects to map to.
//!
//! # Zencan Extensions
//!
//! ## 0x5000 - Auto Start
//!
//! Setting this to a non-zero value causes the node to immediately move into the Operational state
//! after power-on, without receiving an NMT command to do so. Note that, if the device is later put
//! into PreOperational via an NMT command, it will not auto-transition to Operational.
//!
use std::collections::HashMap;

use crate::objects::{AccessType, ObjectCode};
use serde::{de::Error, Deserialize};

use snafu::ResultExt as _;
use snafu::Snafu;

/// Error returned when loading a device config fails
#[derive(Debug, Snafu)]
pub enum LoadError {
    /// An IO error occured while reading the file
    #[snafu(display("IO error: {source}"))]
    Io {
        /// The underlying IO error
        source: std::io::Error,
    },
    /// An error occured in the TOML parser
    #[snafu(display("Toml parse error: {source}"))]
    TomlParsing {
        /// The toml error which led to this error
        source: toml::de::Error,
    },
    /// Multiple objects defined with same index
    #[snafu(display("Multiple definitions for object with index 0x{id:x}"))]
    DuplicateObjectIds {
        /// index which was defined multiple times
        id: u16,
    },
    /// Duplicate sub objects defined on a record
    #[snafu(display("Multiple definitions of sub index {sub} on object 0x{index:x}"))]
    DuplicateSubObjects {
        /// Index of the record object containing duplicate subs
        index: u16,
        /// Duplicated sub index
        sub: u8,
    },
}

fn mandatory_objects(config: &DeviceConfig) -> Vec<ObjectDefinition> {
    vec![
        ObjectDefinition {
            index: 0x1000,
            parameter_name: "Device Type".to_string(),
            application_callback: false,
            object: Object::Var(VarDefinition {
                data_type: DataType::UInt32,
                access_type: AccessType::Const.into(),
                default_value: Some(DefaultValue::Integer(0x00000000)),
                pdo_mapping: PdoMapping::None,
                ..Default::default()
            }),
        },
        ObjectDefinition {
            index: 0x1001,
            parameter_name: "Error Register".to_string(),
            application_callback: false,
            object: Object::Var(VarDefinition {
                data_type: DataType::UInt8,
                access_type: AccessType::Ro.into(),
                default_value: Some(DefaultValue::Integer(0x00000000)),
                pdo_mapping: PdoMapping::None,
                ..Default::default()
            }),
        },
        ObjectDefinition {
            index: 0x1008,
            parameter_name: "Manufacturer Device Name".to_string(),
            application_callback: false,
            object: Object::Var(VarDefinition {
                data_type: DataType::VisibleString(config.device_name.len()),
                access_type: AccessType::Const.into(),
                default_value: Some(DefaultValue::String(config.device_name.clone())),
                pdo_mapping: PdoMapping::None,
                ..Default::default()
            }),
        },
        ObjectDefinition {
            index: 0x1009,
            parameter_name: "Manufacturer Hardware Version".to_string(),
            application_callback: false,
            object: Object::Var(VarDefinition {
                data_type: DataType::VisibleString(config.hardware_version.len()),
                access_type: AccessType::Const.into(),
                default_value: Some(DefaultValue::String(config.hardware_version.clone())),
                pdo_mapping: PdoMapping::None,
                ..Default::default()
            }),
        },
        ObjectDefinition {
            index: 0x100A,
            parameter_name: "Manufacturer Software Version".to_string(),
            application_callback: false,
            object: Object::Var(VarDefinition {
                data_type: DataType::VisibleString(config.software_version.len()),
                access_type: AccessType::Const.into(),
                default_value: Some(DefaultValue::String(config.software_version.clone())),
                pdo_mapping: PdoMapping::None,
                ..Default::default()
            }),
        },
        ObjectDefinition {
            index: 0x1017,
            parameter_name: "Heartbeat Producer Time (ms)".to_string(),
            application_callback: false,
            object: Object::Var(VarDefinition {
                data_type: DataType::UInt16,
                access_type: AccessType::Const.into(),
                default_value: Some(DefaultValue::Integer(config.heartbeat_period as i64)),
                pdo_mapping: PdoMapping::None,
                persist: false,
            }),
        },
        ObjectDefinition {
            index: 0x1018,
            parameter_name: "Identity".to_string(),
            application_callback: false,
            object: Object::Record(RecordDefinition {
                subs: vec![
                    SubDefinition {
                        sub_index: 1,
                        parameter_name: "Vendor ID".to_string(),
                        field_name: Some("vendor_id".into()),
                        data_type: DataType::UInt32,
                        access_type: AccessType::Const.into(),
                        default_value: Some(DefaultValue::Integer(
                            config.identity.vendor_id as i64,
                        )),
                        pdo_mapping: PdoMapping::None,
                        ..Default::default()
                    },
                    SubDefinition {
                        sub_index: 2,
                        parameter_name: "Product Code".to_string(),
                        field_name: Some("product_code".into()),
                        data_type: DataType::UInt32,
                        access_type: AccessType::Const.into(),
                        default_value: Some(DefaultValue::Integer(
                            config.identity.product_code as i64,
                        )),
                        pdo_mapping: PdoMapping::None,
                        ..Default::default()
                    },
                    SubDefinition {
                        sub_index: 3,
                        parameter_name: "Revision Number".to_string(),
                        field_name: Some("revision".into()),
                        data_type: DataType::UInt32,
                        access_type: AccessType::Const.into(),
                        default_value: Some(DefaultValue::Integer(
                            config.identity.revision_number as i64,
                        )),
                        pdo_mapping: PdoMapping::None,
                        ..Default::default()
                    },
                    SubDefinition {
                        sub_index: 4,
                        parameter_name: "Serial Number".to_string(),
                        field_name: Some("serial".into()),
                        data_type: DataType::UInt32,
                        access_type: AccessType::Const.into(),
                        default_value: Some(DefaultValue::Integer(0)),
                        pdo_mapping: PdoMapping::None,
                        ..Default::default()
                    },
                ],
            }),
        },
        ObjectDefinition {
            index: 0x5000,
            parameter_name: "Auto Start".to_string(),
            application_callback: false,
            object: Object::Var(VarDefinition {
                data_type: DataType::UInt8,
                access_type: AccessType::Rw.into(),
                default_value: None,
                pdo_mapping: PdoMapping::None,
                persist: true,
            }),
        },
    ]
}

fn pdo_objects(num_rpdo: usize, num_tpdo: usize) -> Vec<ObjectDefinition> {
    let mut objects = Vec::new();

    fn add_objects(objects: &mut Vec<ObjectDefinition>, i: usize, tx: bool) {
        let pdo_type = if tx { "TPDO" } else { "RPDO" };
        let comm_index = if tx { 0x1800 } else { 0x1400 };
        let mapping_index = if tx { 0x1A00 } else { 0x1600 };

        objects.push(ObjectDefinition {
            index: comm_index + i as u16,
            parameter_name: format!("{}{} Communication Parameter", pdo_type, i),
            application_callback: true,
            object: Object::Record(RecordDefinition {
                subs: vec![
                    SubDefinition {
                        sub_index: 1,
                        parameter_name: format!("COB-ID for {}{}", pdo_type, i),
                        field_name: None,
                        data_type: DataType::UInt32,
                        access_type: AccessType::Rw.into(),
                        default_value: None,
                        pdo_mapping: PdoMapping::None,
                        persist: true,
                    },
                    SubDefinition {
                        sub_index: 2,
                        parameter_name: format!("Transmission type for {}{}", pdo_type, i),
                        field_name: None,
                        data_type: DataType::UInt8,
                        access_type: AccessType::Rw.into(),
                        default_value: None,
                        pdo_mapping: PdoMapping::None,
                        persist: true,
                    },
                ],
            }),
        });

        let mut mapping_subs = vec![SubDefinition {
            sub_index: 0,
            parameter_name: "Valid Mappings".to_string(),
            field_name: None,
            data_type: DataType::UInt8,
            access_type: AccessType::Rw.into(),
            default_value: Some(DefaultValue::Integer(0)),
            pdo_mapping: PdoMapping::None,
            persist: true,
        }];
        for sub in 1..65 {
            mapping_subs.push(SubDefinition {
                sub_index: sub,
                parameter_name: format!("{}{} Mapping App Object {}", pdo_type, i, sub),
                field_name: None,
                data_type: DataType::UInt32,
                access_type: AccessType::Rw.into(),
                default_value: None,
                pdo_mapping: PdoMapping::None,
                persist: true,
            });
        }

        objects.push(ObjectDefinition {
            index: mapping_index + i as u16,
            parameter_name: format!("{}{} Mapping Parameters", pdo_type, i),
            application_callback: true,
            object: Object::Record(RecordDefinition { subs: mapping_subs }),
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

fn bootloader_objects(cfg: &BootloaderConfig) -> Vec<ObjectDefinition> {
    let mut objects = Vec::new();

    if cfg.sections.is_empty() {
        return objects;
    }
    objects.push(ObjectDefinition {
        index: 0x5500,
        parameter_name: "Bootloader Info".into(),
        application_callback: false,
        object: Object::Record(RecordDefinition {
            subs: vec![
                SubDefinition {
                    sub_index: 1,
                    parameter_name: "Bootloader Config".into(),
                    field_name: Some("config".into()),
                    data_type: DataType::UInt32,
                    access_type: AccessType::Ro.into(),
                    default_value: Some(0.into()),
                    pdo_mapping: PdoMapping::None,
                    persist: false,
                },
                SubDefinition {
                    sub_index: 2,
                    parameter_name: "Number of Section".into(),
                    field_name: Some("num_sections".into()),
                    data_type: DataType::UInt8,
                    access_type: AccessType::Ro.into(),
                    default_value: Some(cfg.sections.len().into()),
                    pdo_mapping: PdoMapping::None,
                    persist: false,
                },
                SubDefinition {
                    sub_index: 3,
                    parameter_name: "Reset to Bootloader Command".into(),
                    field_name: None,
                    data_type: DataType::UInt32,
                    access_type: AccessType::Wo.into(),
                    default_value: None,
                    pdo_mapping: PdoMapping::None,
                    persist: false,
                },
            ],
        }),
    });

    for (i, section) in cfg.sections.iter().enumerate() {
        objects.push(ObjectDefinition {
            index: 0x5510 + i as u16,
            parameter_name: format!("Bootloader Section {i}"),
            application_callback: true,
            object: Object::Record(RecordDefinition {
                subs: vec![
                    SubDefinition {
                        sub_index: 1,
                        parameter_name: "Mode bits".into(),
                        data_type: DataType::UInt8,
                        access_type: AccessType::Const.into(),
                        ..Default::default()
                    },
                    SubDefinition {
                        sub_index: 2,
                        parameter_name: "Section Name".into(),
                        data_type: DataType::VisibleString(0),
                        access_type: AccessType::Const.into(),
                        default_value: Some(section.name.as_str().into()),
                        ..Default::default()
                    },
                    SubDefinition {
                        sub_index: 3,
                        parameter_name: "Section Size".into(),
                        data_type: DataType::UInt32,
                        access_type: AccessType::Const.into(),
                        default_value: Some((section.size as i64).into()),
                        ..Default::default()
                    },
                    SubDefinition {
                        sub_index: 4,
                        parameter_name: "Erase Command".into(),
                        data_type: DataType::UInt8,
                        access_type: AccessType::Wo.into(),
                        ..Default::default()
                    },
                    SubDefinition {
                        sub_index: 5,
                        parameter_name: "Data".into(),
                        data_type: DataType::Domain,
                        access_type: AccessType::Rw.into(),
                        ..Default::default()
                    },
                ],
            }),
        });
    }

    objects
}

fn object_storage_objects(dev: &DeviceConfig) -> Vec<ObjectDefinition> {
    if dev.support_storage {
        vec![ObjectDefinition {
            index: 0x1010,
            parameter_name: "Object Save Command".to_string(),
            application_callback: false,
            object: Object::Array(ArrayDefinition {
                data_type: DataType::UInt32,
                access_type: AccessType::Rw.into(),
                array_size: 1,
                persist: false,
                ..Default::default()
            }),
        }]
    } else {
        vec![]
    }
}

fn default_num_rpdo() -> u8 {
    4
}
fn default_num_tpdo() -> u8 {
    4
}
fn default_true() -> bool {
    true
}

/// Configuration options for PDOs
#[derive(Deserialize, Debug, Clone, Copy)]
pub struct PdoConfig {
    #[serde(default = "default_num_rpdo")]
    /// The number of TX PDO slots available in the device. Defaults to 4.
    pub num_tpdo: u8,
    #[serde(default = "default_num_tpdo")]
    /// The number of RX PDO slots available in the device. Defaults to 4.
    pub num_rpdo: u8,
}

impl Default for PdoConfig {
    fn default() -> Self {
        Self {
            num_tpdo: default_num_tpdo(),
            num_rpdo: default_num_rpdo(),
        }
    }
}

/// The device identity is a unique 128-bit number used for addressing the device on the bus
///
/// The configures the three hardcoded components of the identity. The serial number component of
/// the identity must be set by the application to be unique, e.g. based on a value programmed into
/// non-volatile memory or from a UID register on the MCU.
#[derive(Deserialize, Debug, Default, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    /// The 32-bit vendor ID for this device
    pub vendor_id: u32,
    /// The 32-bit product code for this device
    pub product_code: u32,
    /// The 32-bit revision number for this device
    pub revision_number: u32,
}

/// Enum indicating what PDO mappings a sub object supports
#[derive(Deserialize, Debug, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum PdoMapping {
    /// Cannot be mapped to any PDOs
    #[default]
    None,
    /// Can be mapped only to TPDOs
    Tpdo,
    /// Can be mapped only to RPDOs
    Rpdo,
    /// Can be mapped to any PDO
    Both,
}

impl PdoMapping {
    /// Can be mapped to a TPDO
    pub fn supports_tpdo(&self) -> bool {
        matches!(self, PdoMapping::Tpdo | PdoMapping::Both)
    }

    /// Can be mapped to an RPDO
    pub fn supports_rpdo(&self) -> bool {
        matches!(self, PdoMapping::Rpdo | PdoMapping::Both)
    }
}

/// Configuration object to define a programmable bootloader section
#[derive(Clone, Debug, Deserialize)]
pub struct BootloaderSection {
    /// Name of the section
    pub name: String,
    /// Size of the section
    pub size: u32,
}

/// Configuration of bootloader parameters
#[derive(Clone, Deserialize, Debug, Default)]
pub struct BootloaderConfig {
    /// If true, this node is an application which supports resetting to a bootloader, rather than a
    /// bootloader implementation
    #[serde(default)]
    pub application: bool,
    /// List of programmable sections
    #[serde(default)]
    pub sections: Vec<BootloaderSection>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
/// Private struct for seserializing device config files
pub struct DeviceConfig {
    /// The name describing the type of device (e.g. a model)
    pub device_name: String,

    /// Enables object storage commands (object 0x1010)
    ///
    /// Default: true
    #[serde(default = "default_true")]
    pub support_storage: bool,

    /// A version describing the hardware
    #[serde(default)]
    pub hardware_version: String,
    /// A version describing the software
    #[serde(default)]
    pub software_version: String,

    /// The period at which to transmit heartbeat messages in milliseconds
    #[serde(default)]
    pub heartbeat_period: u16,

    /// Configures the identity object on the device
    pub identity: IdentityConfig,

    /// Configure PDO settings
    #[serde(default)]
    pub pdos: PdoConfig,

    /// Configure bootloader options
    #[serde(default)]
    pub bootloader: BootloaderConfig,

    /// A list of application specific objects to define on the device
    #[serde(default)]
    pub objects: Vec<ObjectDefinition>,
}

/// Defines a sub-object in a record
#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct SubDefinition {
    /// Sub index for the sub-object being defined
    pub sub_index: u8,
    /// A human readable name for the value stored in this sub-object
    #[serde(default)]
    pub parameter_name: String,
    /// Used to name the struct field associated with this sub object
    ///
    /// This is only applicable to record objects. If no name is provided, the default field name
    /// will be `sub[index]`, where index is the uppercase hex representation of the sub index
    #[serde(default)]
    pub field_name: Option<String>,
    /// The data type of the sub object
    pub data_type: DataType,
    /// Access permissions for the sub object
    #[serde(default)]
    pub access_type: AccessTypeDeser,
    /// The default value for the sub object
    #[serde(default)]
    pub default_value: Option<DefaultValue>,
    /// Indicates whether this sub object can be mapped to PDOs
    #[serde(default)]
    pub pdo_mapping: PdoMapping,
    /// Indicates if this sub object should be saved when the save command is sent
    #[serde(default)]
    pub persist: bool,
}

/// An enum to represent object default values
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum DefaultValue {
    /// A default value for integer fields
    Integer(i64),
    /// A default value for float fields
    Float(f64),
    /// A default value for string fields
    String(String),
}

impl From<i64> for DefaultValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for DefaultValue {
    fn from(value: i32) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<usize> for DefaultValue {
    fn from(value: usize) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<f64> for DefaultValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<&str> for DefaultValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

/// An enum representing the different types of objects which can be defined in a device config
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "object_type", rename_all = "lowercase")]
pub enum Object {
    /// A var object is just a single value
    Var(VarDefinition),
    /// An array object is an array of values, all with the same type
    Array(ArrayDefinition),
    /// A record is a collection of sub objects all with different types
    Record(RecordDefinition),
}

/// Descriptor for a var object
#[derive(Default, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VarDefinition {
    /// Indicates the type of data stored in the object
    pub data_type: DataType,
    /// Indicates how this object can be accessed
    pub access_type: AccessTypeDeser,
    /// The default value for this object
    pub default_value: Option<DefaultValue>,
    /// Determines which if type of PDO this object can me mapped to
    #[serde(default)]
    pub pdo_mapping: PdoMapping,
    /// Indicates that this object should be saved
    #[serde(default)]
    pub persist: bool,
}

/// Descriptor for an array object
#[derive(Default, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ArrayDefinition {
    /// The datatype of array fields
    pub data_type: DataType,
    /// Access type for all array fields
    pub access_type: AccessTypeDeser,
    /// The number of elements in the array
    pub array_size: usize,
    /// Default values for all array fields
    pub default_value: Option<Vec<DefaultValue>>,
    #[serde(default)]
    /// Whether fields in this array can be mapped to PDOs
    pub pdo_mapping: PdoMapping,
    #[serde(default)]
    /// Whether this array should be saved to flash on command
    pub persist: bool,
}

/// Descriptor for a record object
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordDefinition {
    /// The sub object definitions for this record object
    #[serde(default)]
    pub subs: Vec<SubDefinition>,
}

/// Descriptor for a domain object
///
/// Not yet implemented
#[derive(Clone, Copy, Deserialize, Debug)]
pub struct DomainDefinition {}

/// Descriptor for an object in the object dictionary
#[derive(Deserialize, Debug, Clone)]
pub struct ObjectDefinition {
    /// The index of the object
    pub index: u16,
    /// A human readable name to describe the contents of the object
    #[serde(default)]
    pub parameter_name: String,
    #[serde(default)]
    /// If true, this object is implemented by an application callback, and no storage will be
    /// allocated for it in the object dictionary.
    pub application_callback: bool,
    /// The descriptor for the object
    #[serde(flatten)]
    pub object: Object,
}

impl ObjectDefinition {
    /// Get the object code specifying the type of this object
    pub fn object_code(&self) -> ObjectCode {
        match self.object {
            Object::Var(_) => ObjectCode::Var,
            Object::Array(_) => ObjectCode::Array,
            Object::Record(_) => ObjectCode::Record,
        }
    }
}

impl DeviceConfig {
    /// Try to read a device config from a file
    pub fn load(config_path: impl AsRef<std::path::Path>) -> Result<Self, LoadError> {
        let config_str = std::fs::read_to_string(&config_path).context(IoSnafu)?;
        Self::load_from_str(&config_str)
    }

    /// Try to read a config from a &str
    pub fn load_from_str(config_str: &str) -> Result<Self, LoadError> {
        let mut config: DeviceConfig = toml::from_str(config_str).context(TomlParsingSnafu)?;

        // Add mandatory objects to the config
        config.objects.extend(mandatory_objects(&config));
        config
            .objects
            .extend(bootloader_objects(&config.bootloader));
        config.objects.extend(pdo_objects(
            config.pdos.num_rpdo as usize,
            config.pdos.num_tpdo as usize,
        ));
        config.objects.extend(object_storage_objects(&config));

        Self::validate_unique_indices(&config.objects)?;

        Ok(config)
    }

    fn validate_unique_indices(objects: &[ObjectDefinition]) -> Result<(), LoadError> {
        let mut found_indices = HashMap::new();
        for obj in objects {
            if found_indices.contains_key(&obj.index) {
                return DuplicateObjectIdsSnafu { id: obj.index }.fail();
            }
            found_indices.insert(&obj.index, ());

            if let Object::Record(record) = &obj.object {
                let mut found_subs = HashMap::new();
                for sub in &record.subs {
                    if found_subs.contains_key(&sub.sub_index) {
                        return DuplicateSubObjectsSnafu {
                            index: obj.index,
                            sub: sub.sub_index,
                        }
                        .fail();
                    }
                    found_subs.insert(&sub.sub_index, ());
                }
            }
        }

        Ok(())
    }
}

/// A newtype on AccessType to implement serialization
#[derive(Clone, Copy, Debug, Default)]
pub struct AccessTypeDeser(pub AccessType);
impl<'de> serde::Deserialize<'de> for AccessTypeDeser {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "ro" => Ok(AccessTypeDeser(AccessType::Ro)),
            "rw" => Ok(AccessTypeDeser(AccessType::Rw)),
            "wo" => Ok(AccessTypeDeser(AccessType::Wo)),
            "const" => Ok(AccessTypeDeser(AccessType::Const)),
            _ => Err(D::Error::custom(format!(
                "Invalid access type: {} (allowed: 'ro', 'rw', 'wo', or 'const')",
                s
            ))),
        }
    }
}
impl From<AccessType> for AccessTypeDeser {
    fn from(access_type: AccessType) -> Self {
        AccessTypeDeser(access_type)
    }
}

/// A type to represent data_type fields in a device config
///
/// This is similar, but slightly different from the DataType defined in `zencan_common`
#[derive(Clone, Copy, Debug, Default)]
#[allow(missing_docs)]
pub enum DataType {
    Boolean,
    Int8,
    Int16,
    Int32,
    #[default]
    UInt8,
    UInt16,
    UInt32,
    Real32,
    VisibleString(usize),
    OctetString(usize),
    UnicodeString(usize),
    TimeOfDay,
    TimeDifference,
    Domain,
}

impl DataType {
    /// Returns true if the type is one of the stringy types
    pub fn is_str(&self) -> bool {
        matches!(
            self,
            DataType::VisibleString(_) | DataType::OctetString(_) | DataType::UnicodeString(_)
        )
    }

    /// Get the storage size of the data type
    pub fn size(&self) -> usize {
        match self {
            DataType::Boolean => 1,
            DataType::Int8 => 1,
            DataType::Int16 => 2,
            DataType::Int32 => 4,
            DataType::UInt8 => 1,
            DataType::UInt16 => 2,
            DataType::UInt32 => 4,
            DataType::Real32 => 4,
            DataType::VisibleString(size) => *size,
            DataType::OctetString(size) => *size,
            DataType::UnicodeString(size) => *size,
            DataType::TimeOfDay => 4,
            DataType::TimeDifference => 4,
            DataType::Domain => 0, // Domain size is variable
        }
    }
}

impl<'de> serde::Deserialize<'de> for DataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let re_visiblestring = regex::Regex::new(r"^visiblestring\((\d+)\)$").unwrap();
        let re_octetstring = regex::Regex::new(r"^octetstring\((\d+)\)$").unwrap();
        let re_unicodestring = regex::Regex::new(r"^unicodestring\((\d+)\)$").unwrap();

        let s = String::deserialize(deserializer)?.to_lowercase();
        if s == "boolean" {
            Ok(DataType::Boolean)
        } else if s == "int8" {
            return Ok(DataType::Int8);
        } else if s == "int16" {
            return Ok(DataType::Int16);
        } else if s == "int32" {
            return Ok(DataType::Int32);
        } else if s == "uint8" {
            return Ok(DataType::UInt8);
        } else if s == "uint16" {
            return Ok(DataType::UInt16);
        } else if s == "uint32" {
            return Ok(DataType::UInt32);
        } else if s == "real32" {
            return Ok(DataType::Real32);
        } else if let Some(caps) = re_visiblestring.captures(&s) {
            let size: usize = caps[1].parse().map_err(|_| {
                D::Error::custom(format!("Invalid size for VisibleString: {}", &caps[1]))
            })?;
            return Ok(DataType::VisibleString(size));
        } else if let Some(caps) = re_octetstring.captures(&s) {
            let size: usize = caps[1].parse().map_err(|_| {
                D::Error::custom(format!("Invalid size for OctetString: {}", &caps[1]))
            })?;
            return Ok(DataType::OctetString(size));
        } else if let Some(caps) = re_unicodestring.captures(&s) {
            let size: usize = caps[1].parse().map_err(|_| {
                D::Error::custom(format!("Invalid size for UnicodeString: {}", &caps[1]))
            })?;
            return Ok(DataType::UnicodeString(size));
        } else if s == "timeofday" {
            return Ok(DataType::TimeOfDay);
        } else if s == "timedifference" {
            return Ok(DataType::TimeDifference);
        } else if s == "domain" {
            return Ok(DataType::Domain);
        } else {
            return Err(D::Error::custom(format!("Invalid data type: {}", s)));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::device_config::{DeviceConfig, LoadError};
    use assertables::assert_contains;
    #[test]
    fn test_duplicate_objects_errors() {
        const TOML: &str = r#"
            device_name = "test"
            [identity]
            vendor_id = 0
            product_code = 1
            revision_number = 2

            [[objects]]
            index = 0x2000
            parameter_name = "Test1"
            object_type = "var"
            data_type = "int16"
            access_type = "rw"

            [[objects]]
            index = 0x2000
            parameter_name = "Duplicate"
            object_type = "record"
        "#;

        let result = DeviceConfig::load_from_str(TOML);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LoadError::DuplicateObjectIds { id: 0x2000 }));
        assert_contains!(
            "Multiple definitions for object with index 0x2000",
            err.to_string().as_str()
        );
    }

    #[test]
    fn test_duplicate_sub_object_errors() {
        const TOML: &str = r#"
            device_name = "test"
            [identity]
            vendor_id = 0
            product_code = 1
            revision_number = 2


            [[objects]]
            index = 0x2000
            parameter_name = "Duplicate"
            object_type = "record"
            [[objects.subs]]
            sub_index = 1
            parameter_name = "Test1"
            data_type = "int16"
            access_type = "rw"
            [[objects.subs]]
            sub_index = 1
            parameter_name = "RepeatedTest1"
            data_type = "int16"
            access_type = "rw"
        "#;

        let result = DeviceConfig::load_from_str(TOML);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            LoadError::DuplicateSubObjects {
                index: 0x2000,
                sub: 1
            }
        ));
        assert_contains!(
            "Multiple definitions of sub index 1 on object 0x2000",
            err.to_string().as_str()
        );
    }
}
