use ini::{Ini, Properties};
use snafu::{ResultExt as _, Snafu};
use std::{collections::HashMap, path::Path};

use zencan_common::objects::{AccessType, DataType, ObjectCode};

#[derive(Debug, Snafu)]
pub enum LoadError {
    IniFormatError {
        message: String,
    },
    EdsFormatError {
        message: String,
    },
    ParseIntError {
        message: String,
        source: std::num::ParseIntError,
    },
}

#[derive(Clone, Debug, Default)]
pub struct ElectronicDataSheet {
    pub file_info: FileInfo,
    pub device_info: DeviceInfo,
    pub dummy_usage: DummyUsage,
    pub mandatory_objects: Vec<Object>,
    pub optional_objects: Vec<Object>,
    pub manufacturer_objects: Vec<Object>,
}

#[derive(Clone, Debug, Default)]
pub struct FileInfo {
    pub file_name: String,
    pub file_version: u8,
    pub file_revision: u8,
    pub eds_version: String,
    pub description: String,
    pub creation_time: String,
    pub creation_date: String,
    pub created_by: String,
    pub modification_time: String,
    pub modification_date: String,
    pub modified_by: String,
}

#[derive(Clone, Debug, Default)]
pub struct DeviceInfo {
    pub vendor_name: String,
    pub vendor_number: Option<u32>,
    pub product_name: String,
    pub product_number: Option<u32>,
    pub revision_number: u32,
    pub order_code: String,
    pub baudrate_10: bool,
    pub baudrate_20: bool,
    pub baudrate_50: bool,
    pub baudrate_125: bool,
    pub baudrate_250: bool,
    pub baudrate_500: bool,
    pub baudrate_800: bool,
    pub baudrate_1000: bool,
    pub simple_boot_up_master: bool,
    pub simple_boot_up_slave: bool,
    pub granularity: u32,
    pub dynamic_channels_supported: u32,
    pub group_messaging: bool,
    pub rpdo_count: u16,
    pub tpdo_count: u16,
    pub lss_supported: bool,
    pub ng_slave: bool,
    pub ng_master: bool,
}

#[derive(Clone, Debug, Default)]
pub struct DummyUsage {
    pub values: HashMap<DataType, bool>,
}

fn str_to_access_type(s: &str) -> Result<AccessType, LoadError> {
    let s = s.to_lowercase();
    match s.as_str() {
        "ro" => Ok(AccessType::Ro),
        "wo" => Ok(AccessType::Wo),
        "rw" => Ok(AccessType::Rw),
        "const" => Ok(AccessType::Const),
        _ => EdsFormatSnafu {
            message: format!("Invalid AccessType: '{}'", s),
        }
        .fail(),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Object {
    pub parameter_name: String,
    pub object_number: u32,
    pub object_code: ObjectCode,
    pub subs: HashMap<u8, SubObject>,
    pub sub_number: u8,
}

#[derive(Clone, Debug, Default)]
pub struct SubObject {
    pub data_type: DataType,
    pub access_type: AccessType,
    pub low_limit: Option<String>,
    pub high_limit: Option<String>,
    pub default_value: String,
    /// True if this object can be mapped into a PDO
    pub pdo_mapping: bool,
}

struct Section<'a> {
    properties: &'a Properties,
    name: String,
}

trait ParseHex {
    fn parse_hex(&self) -> Result<u32, std::num::ParseIntError>;
}

impl<T: AsRef<str>> ParseHex for T {
    fn parse_hex(&self) -> Result<u32, std::num::ParseIntError> {
        let s = self.as_ref();
        u32::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16)
    }
}

trait ParseOct {
    fn parse_oct(&self) -> Result<u32, std::num::ParseIntError>;
}

impl<T: AsRef<str>> ParseOct for T {
    fn parse_oct(&self) -> Result<u32, std::num::ParseIntError> {
        let s = self.as_ref();
        u32::from_str_radix(s.strip_prefix("0").unwrap_or(s), 8)
    }
}

impl<'a> Section<'a> {
    pub fn from_name(ini: &'a Ini, name: &str) -> Result<Section<'a>, LoadError> {
        let properties = ini.section(Some(name)).ok_or(
            IniFormatSnafu {
                message: format!("Missing section '{}'", name),
            }
            .build(),
        )?;
        Ok(Section {
            properties,
            name: name.to_string(),
        })
    }

    /// Read a field as a String
    ///
    /// Get the string stored in the field.
    /// Returns Ok(None) if the field is empty.
    /// Returns an error if the field is missing.
    pub fn get_string_opt(&self, field: &str) -> Result<Option<String>, LoadError> {
        match self.properties.get(field) {
            Some(v) => {
                if v.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(v.to_string()))
                }
            }
            None => EdsFormatSnafu {
                message: format!("Missing required field '{}' in '{}'", field, self.name),
            }
            .fail(),
        }
    }

    /// Read a field as a String
    ///
    /// Get the string stored in the field (empty string when empty field).
    /// Returns an error if the field is missing.
    pub fn get_string(&self, field: &str) -> Result<String, LoadError> {
        match self.get_string_opt(field)? {
            Some(v) => Ok(v),
            None => Ok("".to_string()),
        }
    }

    /// Read a field as an unsigned int
    ///
    /// Get the integer value (from decimal, hexadecimal or octal format) stored in a field.
    /// Returns Ok(None) if the field is empty.
    /// Returns an error if the field is missing or if the value can't be parsed to a integer.
    pub fn get_u32_opt(&self, field: &str) -> Result<Option<u32>, LoadError> {
        match self.get_string_opt(field)? {
            Some(v) => {
                let parse_err = ParseIntSnafu {
                    message: format!(
                        "Parsing '{}' from field '{}' in section '{}'",
                        v, field, self.name
                    ),
                };
                if v.starts_with("0x") {
                    v.parse_hex().map(|i| Some(i)).context(parse_err)
                } else if v.starts_with("0") && v != "0" {
                    v.parse_oct().map(|i| Some(i)).context(parse_err)
                } else {
                    v.parse::<u32>().map(|i| Some(i)).context(parse_err)
                }
            }
            None => Ok(None),
        }
    }

    /// Read a field as an unsigned int
    ///
    /// Get the integer value (from decimal, hexadecimal or octal format) stored in a field.
    /// Returns an error if the field is missing or empty, or if the value can't be parsed to a integer.
    pub fn get_u32(&self, field: &str) -> Result<u32, LoadError> {
        self.get_u32_opt(field)?.ok_or(
            EdsFormatSnafu {
                message: format!("Empty field '{}' in '{}'", field, self.name),
            }
            .build(),
        )
    }

    /// Read a field as a boolean
    ///
    /// Get the boolean value stored in the field.
    /// Returns Ok(None) if the field is empty.
    /// Returns an error if the field is missing or if the value can't be parsed to a boolean.
    pub fn _get_bool_opt(&self, field: &str) -> Result<Option<bool>, LoadError> {
        // Boolean is stored as 0 or 1
        // Read as u32, and cast
        self.get_u32_opt(field).map(|v| v.map(|i| i == 1))
    }

    /// Read a field as a boolean
    ///
    /// Get the boolean value stored in the field.
    /// Returns an error if the field is missing or empty, or if the value can't be parsed to a boolean.
    pub fn get_bool(&self, field: &str) -> Result<bool, LoadError> {
        // Boolean is stored as 0 or 1
        // Read as u32, and cast
        self.get_u32(field).map(|i| i == 1)
    }
}

fn get_sub_object(section: &Section) -> Result<SubObject, LoadError> {
    Ok(SubObject {
        data_type: DataType::from(section.get_u32("DataType")? as u16),
        access_type: str_to_access_type(&section.get_string("AccessType")?)?,
        low_limit: section.get_string("LowLimit").ok(),
        high_limit: section.get_string("HighLimit").ok(),
        default_value: section.get_string("DefaultValue")?,
        pdo_mapping: section.get_bool("PDOMapping")?,
    })
}

fn read_object_list(ini: &Ini, name: &str) -> Result<Vec<Object>, LoadError> {
    let mut list = Vec::new();
    let obj_section = Section::from_name(&ini, name)?;
    let num_objects = obj_section.get_u32("SupportedObjects")?;
    for i in 1..num_objects + 1 {
        let obj_num = obj_section.get_u32(&i.to_string())?;
        let obj_section = Section::from_name(&ini, &format!("{:x}", obj_num))?;
        let sub_number = obj_section.get_u32_opt("SubNumber")?.unwrap_or(0) as u8;
        let parameter_name = obj_section.get_string("ParameterName")?;
        let object_code_u8 = obj_section.get_u32("ObjectType")? as u8;
        let object_code = match ObjectCode::try_from(object_code_u8) {
            Ok(value) => Ok(value),
            Err(_) => EdsFormatSnafu {
                message: format!("Invalid object code '{}' in '{}'", object_code_u8, obj_num),
            }
            .fail(),
        }?;
        if sub_number == 0 {
            // There are no explicit subobjects; the top level config dict describes both the
            // top-level object and sub-object 0
            let object = Object {
                object_number: obj_num,
                parameter_name,
                object_code,
                sub_number,
                subs: HashMap::from([(0, get_sub_object(&obj_section)?)]),
            };
            list.push(object);
        } else {
            // There are multiple sub objects
            let mut object = Object {
                object_number: obj_num,
                parameter_name,
                object_code,
                sub_number,
                subs: HashMap::new(),
            };
            for sub_num in 0..255 {
                let sub_section =
                    Section::from_name(&ini, &format!("{:x}sub{:x}", obj_num, sub_num));
                if sub_section.is_err() {
                    // Not all subs are necessarily defined; e.g. there may be a sub1 and a sub3,
                    // but no sub2
                    continue;
                }
                let sub_section = sub_section.unwrap();
                object
                    .subs
                    .insert(sub_num as u8, get_sub_object(&sub_section)?);
                if object.subs.len() == sub_number as usize {
                    break;
                }
            }
            list.push(object);
        }
    }

    Ok(list)
}

impl ElectronicDataSheet {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<ElectronicDataSheet, LoadError> {
        let ini = Ini::load_from_file(path).map_err(|_| {
            IniFormatSnafu {
                message: "Unable to load init file",
            }
            .build()
        })?;
        Self::from_ini(ini)
    }

    pub fn from_str(s: &str) -> Result<ElectronicDataSheet, LoadError> {
        let ini = Ini::load_from_str(s).map_err(|_| {
            IniFormatSnafu {
                message: "Unable to load init file",
            }
            .build()
        })?;
        Self::from_ini(ini)
    }

    fn from_ini(ini: Ini) -> Result<ElectronicDataSheet, LoadError> {
        let file_info_section = Section::from_name(&ini, "FileInfo")?;
        let file_info = FileInfo {
            file_name: file_info_section.get_string("FileName")?,
            file_version: file_info_section.get_u32("FileVersion")? as u8,
            file_revision: file_info_section.get_u32("FileRevision")? as u8,
            eds_version: file_info_section.get_string("EDSVersion")?,
            description: file_info_section.get_string("Description")?,
            creation_time: file_info_section.get_string("CreationTime")?,
            creation_date: file_info_section.get_string("CreationDate")?,
            created_by: file_info_section.get_string("CreatedBy")?,
            modification_time: file_info_section.get_string("ModificationTime")?,
            modification_date: file_info_section.get_string("ModificationDate")?,
            modified_by: file_info_section.get_string("ModifiedBy")?,
        };

        let device_info_section = Section::from_name(&ini, "DeviceInfo")?;
        let device_info = DeviceInfo {
            vendor_name: device_info_section.get_string("VendorName")?,
            vendor_number: device_info_section.get_u32_opt("VendorNumber")?,
            product_name: device_info_section.get_string("ProductName")?,
            product_number: device_info_section.get_u32_opt("ProductNumber")?,
            revision_number: device_info_section.get_u32("RevisionNumber")?,
            order_code: device_info_section.get_string("OrderCode")?,
            baudrate_10: device_info_section.get_bool("BaudRate_10")?,
            baudrate_20: device_info_section.get_bool("BaudRate_20")?,
            baudrate_50: device_info_section.get_bool("BaudRate_50")?,
            baudrate_125: device_info_section.get_bool("BaudRate_125")?,
            baudrate_250: device_info_section.get_bool("BaudRate_250")?,
            baudrate_500: device_info_section.get_bool("BaudRate_500")?,
            baudrate_800: device_info_section.get_bool("BaudRate_800")?,
            baudrate_1000: device_info_section.get_bool("BaudRate_1000")?,
            simple_boot_up_master: device_info_section.get_bool("SimpleBootUpMaster")?,
            simple_boot_up_slave: device_info_section.get_bool("SimpleBootUpSlave")?,
            granularity: device_info_section.get_u32("Granularity")?,
            dynamic_channels_supported: device_info_section.get_u32("DynamicChannelsSupported")?,
            group_messaging: device_info_section.get_bool("GroupMessaging")?,
            rpdo_count: device_info_section.get_u32("NrOfRXPDO")? as u16,
            tpdo_count: device_info_section.get_u32("NrOfTXPDO")? as u16,
            lss_supported: device_info_section.get_bool("LSS_Supported")?,
            ng_slave: device_info_section.get_bool("NG_Slave").unwrap_or(false),
            ng_master: device_info_section
                .get_bool("LSS_Supported")
                .unwrap_or(false),
        };

        let dummy_usage_section = Section::from_name(&ini, "DummyUsage")?;
        let mut dummy_usage = DummyUsage::default();
        for (k, v) in dummy_usage_section.properties.iter() {
            let suffix = match k.strip_prefix("Dummy") {
                Some(value) => Ok(value),
                None => EdsFormatSnafu {
                    message: format!(
                        "Invalid field format '{}' in '{}'",
                        k, dummy_usage_section.name
                    ),
                }
                .fail(),
            }?
            .parse()
            .unwrap_or(0);
            let data_type = match DataType::try_from(suffix) {
                Ok(value) => Ok(value),
                Err(_) => EdsFormatSnafu {
                    message: format!(
                        "Invalid field format '{}' in '{}'",
                        k, dummy_usage_section.name
                    ),
                }
                .fail(),
            }?;

            let supported = v.parse::<u32>().context(ParseIntSnafu {
                message: format!("Parsing '{}' in section '{}'", k, "DummyUsage"),
            })? == 1;
            dummy_usage.values.insert(data_type, supported);
        }

        Ok(ElectronicDataSheet {
            file_info,
            device_info,
            dummy_usage,
            mandatory_objects: read_object_list(&ini, "MandatoryObjects")?,
            optional_objects: read_object_list(&ini, "OptionalObjects")?,
            manufacturer_objects: read_object_list(&ini, "ManufacturerObjects")?,
        })
    }
}

#[cfg(test)]
mod tests {
    // use std::io::Write;

    // use super::*;

    // #[test]
    // fn test_load() {
    //     const EDS: &[u8] = include_bytes!("example.eds");

    //     let mut eds_file = tempfile::NamedTempFile::new().unwrap();
    //     eds_file.write_all(EDS).unwrap();

    //     let eds = ElectronicDataSheet::load(eds_file.path()).unwrap();
    //     println!("Eds: {:?}", eds);
    //     assert!(false, "EDS loaded; just failing to read the output");
    // }
}
