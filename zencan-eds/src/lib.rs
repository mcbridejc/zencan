use ini::{Ini, Properties};
use snafu::{ResultExt as _, Snafu};
use std::{collections::BTreeMap, path::Path};

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
    pub comments: Comments,
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
    pub revision_number: Option<u32>,
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
}

#[derive(Clone, Debug, Default)]
pub struct DummyUsage {
    pub values: BTreeMap<DataType, bool>,
}

#[derive(Clone, Debug, Default)]
pub struct Comments {
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Object {
    pub parameter_name: String,
    pub object_number: u16,
    pub object_code: ObjectCode,
    pub sub_number: u8,
    pub subs: BTreeMap<u8, SubObject>,
}

#[derive(Clone, Debug, Default)]
pub struct SubObject {
    pub parameter_name: String,
    pub data_type: DataType,
    pub access_type: AccessType,
    pub low_limit: Option<String>,
    pub high_limit: Option<String>,
    pub default_value: Option<String>,
    /// True if this object can be mapped into a PDO
    pub pdo_mapping: Option<bool>,
}

impl Object {
    pub fn new(
        parameter_name: String,
        object_number: u16,
        object_code: ObjectCode,
        sub_number: u8,
        subs: BTreeMap<u8, SubObject>,
    ) -> Result<Self, LoadError> {
        use ObjectCode::*;
        match object_code {
            Null => panic!("Null objects are not supported"),
            Domain | Var => Ok(Object {
                parameter_name,
                object_number,
                object_code,
                sub_number,
                subs,
            }),
            DefType | DefStruct => todo!("not yet implemented"),
            Array | Record => {
                if subs.len() != sub_number as usize {
                    return EdsFormatSnafu {
                        message: format!(
                            "Invalid number of subojects in '[{:X}]': expected {}, found {}",
                            object_number,
                            sub_number,
                            subs.len()
                        ),
                    }
                    .fail();
                }

                let (first_subindex, first_subobj) = subs.first_key_value().ok_or(
                    EdsFormatSnafu {
                        message: format!(
                            "Unable to extract first subobject of '[{:X}]'",
                            object_number
                        ),
                    }
                    .build(),
                )?;
                if *first_subindex != 0 {
                    return EdsFormatSnafu {
                        message: format!(
                            "Invalid subindex for first subobject of '[{:X}]': expected 0, found {}",
                            object_number, first_subindex
                        ),
                    }
                    .fail();
                }
                let highest_subindex = first_subobj
                    .default_value
                    .clone()
                    .ok_or(
                        EdsFormatSnafu {
                            message: format!(
                                "Missing required field 'DefaultValue' for first subobject of '[{:X}]'",
                                object_number
                            ),
                        }
                        .build(),
                    )?
                    .parse_hex()
                    .context(ParseIntSnafu {
                        message: format!(
                            "Invalid default value for subobject {} of '[{:X}]'",
                            first_subindex, object_number
                        ),
                    })? as u8;

                let (last_subindex, _) = subs.last_key_value().ok_or(
                    EdsFormatSnafu {
                        message: format!(
                            "Unable to extract last subobject of '[{:X}]'",
                            object_number
                        ),
                    }
                    .build(),
                )?;
                if *last_subindex != highest_subindex {
                    return EdsFormatSnafu {
                        message: format!(
                            "Invalid subindex for last subobject of '[{:X}]': expected highest subindex {}, found {}",
                            object_number, highest_subindex, last_subindex
                        ),
                    }
                    .fail();
                }
                Ok(Object {
                    parameter_name,
                    object_number,
                    object_code,
                    sub_number,
                    subs,
                })
            }
        }
    }
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
        Ok(self.get_string_opt(field)?.unwrap_or_default())
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
                    v.parse_hex().map(Some).context(parse_err)
                } else if v.starts_with("0") && v != "0" {
                    v.parse_oct().map(Some).context(parse_err)
                } else {
                    v.parse::<u32>().map(Some).context(parse_err)
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
        let device_info_section = Section::from_name(&ini, "DeviceInfo")?;
        let dummy_usage_section = Section::from_name(&ini, "DummyUsage")?;
        let comments_section = Section::from_name(&ini, "Comments")?;
        Ok(ElectronicDataSheet {
            file_info: ElectronicDataSheet::parse_file_info(&file_info_section)?,
            device_info: ElectronicDataSheet::parse_device_info(&device_info_section)?,
            dummy_usage: ElectronicDataSheet::parse_dummy_usage(&dummy_usage_section)?,
            comments: ElectronicDataSheet::parse_comments(&comments_section)?,
            mandatory_objects: ElectronicDataSheet::parse_objects(&ini, "MandatoryObjects")?,
            optional_objects: ElectronicDataSheet::parse_objects(&ini, "OptionalObjects")?,
            manufacturer_objects: ElectronicDataSheet::parse_objects(&ini, "ManufacturerObjects")?,
        })
    }

    fn parse_file_info(section: &Section) -> Result<FileInfo, LoadError> {
        Ok(FileInfo {
            file_name: section.get_string("FileName")?,
            file_version: section.get_u32("FileVersion")? as u8,
            file_revision: section.get_u32("FileRevision")? as u8,
            eds_version: section.get_string("EDSVersion")?,
            description: section.get_string("Description")?,
            creation_time: section.get_string("CreationTime")?,
            creation_date: section.get_string("CreationDate")?,
            created_by: section.get_string("CreatedBy")?,
            modification_time: section.get_string("ModificationTime")?,
            modification_date: section.get_string("ModificationDate")?,
            modified_by: section.get_string("ModifiedBy")?,
        })
    }

    fn parse_device_info(section: &Section) -> Result<DeviceInfo, LoadError> {
        Ok(DeviceInfo {
            vendor_name: section.get_string("VendorName")?,
            vendor_number: section.get_u32_opt("VendorNumber")?,
            product_name: section.get_string("ProductName")?,
            product_number: section.get_u32_opt("ProductNumber")?,
            revision_number: section.get_u32_opt("RevisionNumber")?,
            order_code: section.get_string("OrderCode")?,
            baudrate_10: section.get_bool("BaudRate_10")?,
            baudrate_20: section.get_bool("BaudRate_20")?,
            baudrate_50: section.get_bool("BaudRate_50")?,
            baudrate_125: section.get_bool("BaudRate_125")?,
            baudrate_250: section.get_bool("BaudRate_250")?,
            baudrate_500: section.get_bool("BaudRate_500")?,
            baudrate_800: section.get_bool("BaudRate_800")?,
            baudrate_1000: section.get_bool("BaudRate_1000")?,
            simple_boot_up_master: section.get_bool("SimpleBootUpMaster")?,
            simple_boot_up_slave: section.get_bool("SimpleBootUpSlave")?,
            granularity: section.get_u32("Granularity")?,
            dynamic_channels_supported: section.get_u32("DynamicChannelsSupported")?,
            group_messaging: section.get_bool("GroupMessaging")?,
            rpdo_count: section.get_u32("NrOfRXPDO")? as u16,
            tpdo_count: section.get_u32("NrOfTXPDO")? as u16,
            lss_supported: section.get_bool("LSS_Supported")?,
        })
    }

    fn parse_dummy_usage(section: &Section) -> Result<DummyUsage, LoadError> {
        let mut dummy_usage = DummyUsage::default();
        for (k, v) in section.properties.iter() {
            let err = EdsFormatSnafu {
                message: format!("Invalid field format '{}' in '{}'", k, section.name),
            };
            let index = k
                .strip_prefix("Dummy")
                .ok_or(err.clone().build())?
                .parse::<u16>()
                .map_err(|_| err.build())?;
            let supported = v.parse::<u32>().context(ParseIntSnafu {
                message: format!("Parsing '{}' in section '{}'", k, "DummyUsage"),
            })? == 1;
            dummy_usage.values.insert(DataType::from(index), supported);
        }
        Ok(dummy_usage)
    }

    fn parse_comments(section: &Section) -> Result<Comments, LoadError> {
        let line_count = section.get_u32("Lines")? as u16;
        let mut comments = Comments {
            ..Default::default()
        };
        for line_number in 1..line_count + 1 {
            comments
                .lines
                .push(section.get_string(&format!("Line{}", line_number))?);
        }
        Ok(comments)
    }

    fn parse_objects(ini: &Ini, name: &str) -> Result<Vec<Object>, LoadError> {
        let section = Section::from_name(&ini, name)?;
        let objects = ElectronicDataSheet::parse_object_list(&section)?;
        let mut list = Vec::new();
        for index in objects {
            let section = Section::from_name(&ini, &format!("{:X}", index))?;
            list.push(ElectronicDataSheet::parse_object(ini, &section)?);
        }
        Ok(list)
    }

    fn parse_object_list(section: &Section) -> Result<Vec<u32>, LoadError> {
        let count_field = "SupportedObjects";
        let count = section.get_u32_opt(count_field)?.ok_or(
            EdsFormatSnafu {
                message: format!(
                    "Missing required field '{}' in '{}'",
                    count_field, section.name
                ),
            }
            .build(),
        )?;
        let mut indexes = Vec::new();
        for i in 1..count + 1 {
            let index = section.get_u32_opt(&i.to_string())?.ok_or(
                EdsFormatSnafu {
                    message: format!("Missing expected field '{}' in '{}'", i, section.name),
                }
                .build(),
            )?;
            indexes.push(index);
        }
        Ok(indexes)
    }

    fn parse_object(ini: &Ini, section: &Section) -> Result<Object, LoadError> {
        let object_code = match section.get_u32_opt("ObjectType")?.map(|i| i as u8) {
            None => Ok(ObjectCode::Var),
            Some(v) => match ObjectCode::try_from(v) {
                Ok(v) => Ok(v),
                Err(_) => EdsFormatSnafu {
                    message: format!("Invalid object code '{}' in '{}'", v, section.name),
                }
                .fail(),
            },
        }?;

        use ObjectCode::*;
        match object_code {
            Null => EdsFormatSnafu {
                message: format!(
                    "Invalid object code '{}' in '{}'",
                    object_code as u8, section.name
                ),
            }
            .fail(),
            Domain => ElectronicDataSheet::parse_domain(section),
            DefType => todo!("not yet implemented"),
            DefStruct => todo!("not yet implemented"),
            Var => ElectronicDataSheet::parse_var(section),
            Array => ElectronicDataSheet::parse_array(ini, section),
            Record => ElectronicDataSheet::parse_record(ini, section),
        }
    }

    fn parse_domain(section: &Section) -> Result<Object, LoadError> {
        let object_number = section
            .name
            .parse_hex()
            .map(|i| i as u16)
            .context(ParseIntSnafu {
                message: "".to_string(),
            })?;
        Ok(Object::new(
            section.get_string("ParameterName")?,
            object_number,
            ObjectCode::Domain,
            0,
            BTreeMap::new(),
        ))?
    }

    fn parse_var(section: &Section) -> Result<Object, LoadError> {
        let object_number = section
            .name
            .parse_hex()
            .map(|i| i as u16)
            .context(ParseIntSnafu {
                message: "".to_string(),
            })?;
        Ok(Object::new(
            section.get_string("ParameterName")?,
            object_number,
            ObjectCode::Var,
            0,
            BTreeMap::from([(0, ElectronicDataSheet::parse_subobject(section)?)]),
        ))?
    }

    fn parse_array(ini: &Ini, section: &Section) -> Result<Object, LoadError> {
        let sub_number_field = "SubNumber";
        let compact_subobj_field = "CompactSubObj";
        let sub_number = section.get_u32(sub_number_field).unwrap_or(0) as u8;
        let compact_sub_obj = section.get_u32(compact_subobj_field).unwrap_or(0) as u8;
        match (sub_number != 0, compact_sub_obj != 0) {
            (false, false) | (true, true) => EdsFormatSnafu {
                message: format!(
                    "Mismatch between '{}' and '{}' in '{}': expected exactly one of them",
                    sub_number_field, compact_subobj_field, section.name
                ),
            }
            .fail(),
            (false, true) => todo!("compact array is not supported"),
            (true, false) => {
                let object_number =
                    section
                        .name
                        .parse_hex()
                        .map(|i| i as u16)
                        .context(ParseIntSnafu {
                            message: "".to_string(),
                        })?;
                let mut subs = BTreeMap::new();
                for subindex in 0..0xFF {
                    if let Ok(sub_section) =
                        Section::from_name(&ini, &format!("{}sub{:X}", section.name, subindex))
                    {
                        subs.insert(
                            subindex,
                            ElectronicDataSheet::parse_subobject(&sub_section)?,
                        );
                    }
                }
                Ok(Object::new(
                    section.get_string("ParameterName")?,
                    object_number,
                    ObjectCode::Array,
                    sub_number,
                    subs,
                ))?
            }
        }
    }

    fn parse_record(ini: &Ini, section: &Section) -> Result<Object, LoadError> {
        let sub_number_field = "SubNumber";
        let compact_sub_obj_field = "CompactSubObj";
        let sub_number = section.get_u32(sub_number_field).unwrap_or(0) as u8;
        let compact_sub_obj = section.get_u32(compact_sub_obj_field).unwrap_or(0) as u8;
        match (sub_number != 0, compact_sub_obj != 0) {
            (false, false) | (true, true) => EdsFormatSnafu {
                message: format!(
                    "Mismatch between '{}' and '{}' in section '{}': expected exactly one of them",
                    sub_number_field, compact_sub_obj_field, section.name
                ),
            }
            .fail(),
            (false, true) => todo!("compact record is not supported"),
            (true, false) => {
                let object_number = section
                    .name
                    .parse_hex()
                    .map(|i| i as u16)
                    .context(ParseIntSnafu { message: "" })?;
                let mut subs = BTreeMap::new();
                for subindex in 0..0xFF {
                    if let Ok(sub_section) =
                        Section::from_name(&ini, &format!("{}sub{:X}", section.name, subindex))
                    {
                        subs.insert(
                            subindex,
                            ElectronicDataSheet::parse_subobject(&sub_section)?,
                        );
                    }
                }
                Ok(Object::new(
                    section.get_string("ParameterName")?,
                    object_number,
                    ObjectCode::Record,
                    sub_number,
                    subs,
                ))?
            }
        }
    }

    fn parse_subobject(section: &Section) -> Result<SubObject, LoadError> {
        let access_type_field = "AccessType";
        let access_type_raw = section.get_string(access_type_field)?;
        let access_type =
            AccessType::try_from(access_type_raw.to_lowercase().as_ref()).map_err(|_| {
                EdsFormatSnafu {
                    message: format!(
                        "Invalid value '{}' for field '{}' in '{}'",
                        access_type_raw, access_type_field, section.name
                    ),
                }
                .build()
            })?;
        Ok(SubObject {
            parameter_name: section.get_string("ParameterName")?,
            data_type: DataType::from(section.get_u32("DataType")? as u16),
            access_type,
            low_limit: section.get_string("LowLimit").ok(),
            high_limit: section.get_string("HighLimit").ok(),
            default_value: section.get_string("DefaultValue").ok(),
            pdo_mapping: section.get_bool("PDOMapping").ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load() {
        const EDS: &[u8] = include_bytes!("example.eds");

        let mut eds_file = tempfile::NamedTempFile::new().unwrap();
        eds_file.write_all(EDS).unwrap();

        let eds = ElectronicDataSheet::from_file(eds_file.path()).unwrap();
        assert!(eds.manufacturer_objects.is_empty());
        println!("{:#?}", eds);
    }

    #[test]
    fn test_var() {
        let buf = "
[1002]
ParameterName=test
DataType=0x0001
AccessType=ro
";
        let ini = Ini::load_from_str(buf).unwrap();
        let section = Section::from_name(&ini, "1002").unwrap();

        let var = ElectronicDataSheet::parse_var(&section);
        assert!(var.is_ok());
        let var = var.unwrap();
        assert_eq!(var.object_number, 0x1002);
        assert_eq!(var.parameter_name, "test");
        assert_eq!(var.object_code, ObjectCode::Var);
        let sub0 = var.subs.get(&0).unwrap();
        assert_eq!(sub0.data_type, DataType::Boolean);
        assert_eq!(sub0.access_type, AccessType::Ro);

        let arr = ElectronicDataSheet::parse_array(&ini, &section);
        assert!(arr.is_err());

        let rec = ElectronicDataSheet::parse_record(&ini, &section);
        assert!(rec.is_err());
    }
}
