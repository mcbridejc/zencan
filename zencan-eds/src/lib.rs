use configparser::ini::Ini;
use snafu::{ResultExt as _, Snafu};
use std::{collections::HashMap, path::Path};

use zencan_common::objects::{AccessType, DataType};

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

#[derive(Copy, Clone, Debug, Default)]
#[repr(u16)]
pub enum ObjectType {
    #[default]
    Null = 0,
    Var = 7,
    Array = 8,
    Record = 9,
    Unknown(u16),
}

impl From<u16> for ObjectType {
    fn from(value: u16) -> Self {
        use ObjectType::*;
        match value {
            0 => Null,
            7 => Var,
            8 => Array,
            9 => Record,
            _ => Unknown(value),
        }
    }
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
    pub object_type: ObjectType,
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
    map: &'a HashMap<String, Option<String>>,
    section: String,
}

trait ParseHex {
    fn parse_hex(&self) -> Result<u32, std::num::ParseIntError>;
}

impl<T: AsRef<str>> ParseHex for T {
    fn parse_hex(&self) -> Result<u32, std::num::ParseIntError> {
        let s = self.as_ref();
        u32::from_str_radix(s.strip_prefix("0x").unwrap(), 16)
    }
}

impl<'a> Section<'a> {
    pub fn from_map(
        map: &'a HashMap<String, HashMap<String, Option<String>>>,
        section: &str,
    ) -> Result<Self, LoadError> {
        let section_map = match map.get(&section.to_lowercase()) {
            Some(value) => value,
            None => {
                return EdsFormatSnafu {
                    message: format!("Missing required section '{}'", section),
                }
                .fail()
            }
        };
        Ok(Self {
            map: section_map,
            section: section.to_string(),
        })
    }

    pub fn get_string(&self, field: &str) -> Result<String, LoadError> {
        match self.map.get(&field.to_lowercase()) {
            Some(value) => Ok(value.as_ref().unwrap().clone()),
            None => EdsFormatSnafu {
                message: format!("Missing required field '{}' in '{}'", field, self.section),
            }
            .fail(),
        }
    }

    /// Read a field as an unsigned int
    ///
    /// The field must contain a valid integer value or an error is returned
    pub fn get_u32(&self, field: &str) -> Result<u32, LoadError> {
        match self.map.get(&field.to_lowercase()) {
            Some(value) => Ok(value.as_ref().unwrap()),
            None => EdsFormatSnafu {
                message: format!("Missing required field '{}' in '{}'", field, self.section),
            }
            .fail(),
        }?
        .parse()
        .context(ParseIntSnafu {
            message: format!("Parsing '{}' in section '{}'", field, self.section),
        })
    }

    pub fn get_u32_hex(&self, field: &str) -> Result<u32, LoadError> {
        match self.map.get(&field.to_lowercase()) {
            Some(value) => Ok(value.as_ref().unwrap()),
            None => EdsFormatSnafu {
                message: format!("Missing required field '{}' in '{}'", field, self.section),
            }
            .fail(),
        }?
        .parse_hex()
        .context(ParseIntSnafu {
            message: format!("Parsing '{}' in section '{}'", field, self.section),
        })
    }

    pub fn get_u32_hex_opt(&self, field: &str) -> Result<Option<u32>, LoadError> {
        let str_value = match self.map.get(&field.to_lowercase()) {
            Some(value) => Ok(value.as_ref().unwrap()),
            None => return Ok(None),
        }?;

        if str_value.is_empty() {
            return Ok(None);
        }

        Ok(Some(str_value.parse_hex().context(ParseIntSnafu {
            message: format!("Parsing '{}' in section '{}'", field, self.section),
        })?))
    }

    /// Read an optional field as an unsigned int
    ///
    /// If the field is empty, None is returned. If the field has a non-empty value that is not a
    /// valid integer, it will return a LoadError::ParseIntError.
    pub fn get_u32_opt(&self, field: &str) -> Result<Option<u32>, LoadError> {
        let str_value = match self.map.get(&field.to_lowercase()) {
            Some(value) => Ok(value.as_ref().unwrap()),
            None => return Ok(None),
        }?;

        if str_value.is_empty() {
            return Ok(None);
        }

        Ok(Some(str_value.parse().context(ParseIntSnafu {
            message: format!("Parsing '{}' in section '{}'", field, self.section),
        })?))
    }

    pub fn get_bool(&self, field: &str) -> Result<bool, LoadError> {
        // Boolean is stored as 0 or 1
        // Read as u32, and cast
        Ok(self.get_u32(field)? == 1)
    }
}

fn get_sub_object(section: &Section) -> Result<SubObject, LoadError> {
    Ok(SubObject {
        data_type: DataType::from(section.get_u32_hex("DataType")? as u16),
        access_type: str_to_access_type(&section.get_string("AccessType")?)?,
        low_limit: section.get_string("LowLimit").ok(),
        high_limit: section.get_string("HighLimit").ok(),
        default_value: section.get_string("DefaultValue")?,
        pdo_mapping: section.get_bool("PDOMapping")?,
    })
}

fn read_object_list(
    map: &HashMap<String, HashMap<String, Option<String>>>,
    name: &str,
) -> Result<Vec<Object>, LoadError> {
    let mut list = Vec::new();
    let top_section = Section::from_map(map, name)?;
    let num_objects = top_section.get_u32("SupportedObjects")?;
    for i in 1..num_objects + 1 {
        let obj_num = top_section.get_u32_hex(&i.to_string())?;
        let obj_section = Section::from_map(map, &format!("{:x}", obj_num))?;
        let sub_number = obj_section.get_u32_hex_opt("SubNumber")?.unwrap_or(0) as u8;
        let parameter_name = obj_section.get_string("ParameterName")?;
        let object_type = ObjectType::from(obj_section.get_u32_hex("ObjectType")? as u16);
        if sub_number == 0 {
            // There are no explicit subobjects; the top level config dict describes both the
            // top-level object and sub-object 0
            let object = Object {
                object_number: obj_num,
                parameter_name,
                object_type,
                sub_number,
                subs: HashMap::from([(0, get_sub_object(&obj_section)?)]),
            };
            list.push(object);
        } else {
            // There are multiple sub objects
            let mut object = Object {
                object_number: obj_num,
                parameter_name,
                object_type,
                sub_number,
                subs: HashMap::new(),
            };
            for sub_num in 0..255 {
                let sub_section = Section::from_map(map, &format!("{:x}sub{:x}", obj_num, sub_num));
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
    pub fn from_config_map(
        map: &HashMap<String, HashMap<String, Option<String>>>,
    ) -> Result<ElectronicDataSheet, LoadError> {
        let file_info_cfg = Section::from_map(map, "FileInfo")?;

        let file_info = FileInfo {
            file_name: file_info_cfg.get_string("FileName")?,
            file_version: file_info_cfg.get_u32("FileVersion")? as u8,
            file_revision: file_info_cfg.get_u32("FileRevision")? as u8,
            eds_version: file_info_cfg.get_string("EDSVersion")?,
            description: file_info_cfg.get_string("Description")?,
            creation_time: file_info_cfg.get_string("CreationTime")?,
            creation_date: file_info_cfg.get_string("CreationDate")?,
            created_by: file_info_cfg.get_string("CreatedBy")?,
            modification_time: file_info_cfg.get_string("ModificationTime")?,
            modification_date: file_info_cfg.get_string("ModificationDate")?,
            modified_by: file_info_cfg.get_string("ModifiedBy")?,
        };

        let di_cfg = Section::from_map(map, "DeviceInfo")?;
        let device_info = DeviceInfo {
            vendor_name: di_cfg.get_string("VendorName")?,
            vendor_number: di_cfg.get_u32_opt("VendorNumber")?,
            product_name: di_cfg.get_string("ProductName")?,
            product_number: di_cfg.get_u32_opt("ProductNumber")?,
            revision_number: di_cfg.get_u32("RevisionNumber")?,
            order_code: di_cfg.get_string("OrderCode")?,
            baudrate_10: di_cfg.get_bool("BaudRate_10")?,
            baudrate_20: di_cfg.get_bool("BaudRate_20")?,
            baudrate_50: di_cfg.get_bool("BaudRate_50")?,
            baudrate_125: di_cfg.get_bool("BaudRate_125")?,
            baudrate_250: di_cfg.get_bool("BaudRate_250")?,
            baudrate_500: di_cfg.get_bool("BaudRate_500")?,
            baudrate_800: di_cfg.get_bool("BaudRate_800")?,
            baudrate_1000: di_cfg.get_bool("BaudRate_1000")?,
            simple_boot_up_master: di_cfg.get_bool("SimpleBootUpMaster")?,
            simple_boot_up_slave: di_cfg.get_bool("SimpleBootUpSlave")?,
            granularity: di_cfg.get_u32("Granularity")?,
            dynamic_channels_supported: di_cfg.get_u32("DynamicChannelsSupported")?,
            group_messaging: di_cfg.get_bool("GroupMessaging")?,
            rpdo_count: di_cfg.get_u32("NrOfRXPDO")? as u16,
            tpdo_count: di_cfg.get_u32("NrOfTXPDO")? as u16,
            lss_supported: di_cfg.get_bool("LSS_Supported")?,
            ng_slave: di_cfg.get_bool("NG_Slave").unwrap_or(false),
            ng_master: di_cfg.get_bool("LSS_Supported").unwrap_or(false),
        };

        Ok(ElectronicDataSheet {
            file_info,
            device_info,
            mandatory_objects: read_object_list(map, "MandatoryObjects")?,
            optional_objects: read_object_list(map, "OptionalObjects")?,
            manufacturer_objects: read_object_list(map, "ManufacturerObjects")?,
        })
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str<S: Into<String>>(eds_file: S) -> Result<ElectronicDataSheet, LoadError> {
        let s = eds_file.into();
        let mut config = Ini::new();
        let map = config
            .read(s)
            .map_err(|e| IniFormatSnafu { message: e }.build())?;
        Self::from_config_map(&map)
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<ElectronicDataSheet, LoadError> {
        let mut config = Ini::new();
        let map = config
            .load(path)
            .map_err(|e| IniFormatSnafu { message: e }.build())?;
        Self::from_config_map(&map)
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
