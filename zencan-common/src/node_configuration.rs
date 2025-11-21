//! Node Configuration File Format
use std::{collections::HashMap, path::Path};

use crate::{pdo::PdoMapping, CanId};
use serde::{de, Deserialize, Deserializer};
use snafu::{ResultExt, Snafu};

/// Error returned when loading node configuration files
#[derive(Debug, Snafu)]
pub enum ConfigError {
    /// An IO error
    #[snafu(display("IO error loading {path}: {source:?}"))]
    Io {
        /// The path being accessed
        path: String,
        /// The original error
        source: std::io::Error,
    },
    /// A TOML error
    #[snafu(display("Error parsing TOML: {source}"))]
    TomlDeserialization {
        /// The original error
        source: toml::de::Error,
    },
}

/// Represents a store command to write a value to an object
#[derive(Clone, Debug, PartialEq)]
pub struct Store {
    /// Index of the object to be written
    pub index: u16,
    /// Sub index to be written
    pub sub: u8,
    /// The value to be written to the sub object
    pub value: StoreValue,
}

impl Store {
    /// Get the value as bytes
    pub fn raw_value(&self) -> Vec<u8> {
        self.value.raw()
    }
}

/// Value to be stored by a [Store] command
#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub enum StoreValue {
    U32(u32),
    U16(u16),
    U8(u8),
    I32(i32),
    I16(i16),
    I8(i8),
    F32(f32),
    String(String),
}

impl StoreValue {
    /// Get the value as bytes
    pub fn raw(&self) -> Vec<u8> {
        match self {
            StoreValue::U32(v) => v.to_le_bytes().to_vec(),
            StoreValue::U16(v) => v.to_le_bytes().to_vec(),
            StoreValue::U8(v) => vec![*v],
            StoreValue::I32(v) => v.to_le_bytes().to_vec(),
            StoreValue::I16(v) => v.to_le_bytes().to_vec(),
            StoreValue::I8(v) => vec![*v as u8],
            StoreValue::F32(v) => v.to_le_bytes().to_vec(),
            StoreValue::String(ref s) => s.as_bytes().to_vec(),
        }
    }
}

/// A node configuration
///
/// Represents a runtime configuration which can be loaded into a node
///
/// It describes the configuration of PDOs, and other arbitrary objects on the node
#[derive(Debug, Clone)]
pub struct NodeConfig(NodeConfigSerializer);

impl NodeConfig {
    /// Read a configuration from a file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<NodeConfig, ConfigError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).context(IoSnafu {
            path: path.to_string_lossy(),
        })?;
        Self::load_from_str(&content)
    }

    /// Read a configuration from a string
    pub fn load_from_str(s: &str) -> Result<NodeConfig, ConfigError> {
        let raw_config: NodeConfigSerializer =
            toml::from_str(s).context(TomlDeserializationSnafu)?;

        Ok(NodeConfig(raw_config))
    }

    /// Get the transmit PDO configurations
    pub fn tpdos(&self) -> &HashMap<usize, PdoConfig> {
        &self.0.tpdo.0
    }

    /// Get the receive PDO configurations
    pub fn rpdos(&self) -> &HashMap<usize, PdoConfig> {
        &self.0.rpdo.0
    }

    /// Get the object configurations
    ///
    /// Each store represents a value to be written to a specific sub object during configuration
    pub fn stores(&self) -> &[Store] {
        &self.0.store
    }
}

#[derive(Clone, Debug, Default, Deserialize)]

pub(crate) struct PdoConfigMapSerializer(
    #[serde(deserialize_with = "deserialize_pdo_map", default)] pub HashMap<usize, PdoConfig>,
);

impl From<PdoConfigMapSerializer> for HashMap<usize, PdoConfig> {
    fn from(value: PdoConfigMapSerializer) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeConfigSerializer {
    #[serde(default)]
    pub tpdo: PdoConfigMapSerializer,
    #[serde(default)]
    pub rpdo: PdoConfigMapSerializer,
    #[serde(default, deserialize_with = "deserialize_store")]
    pub store: Vec<Store>,
}

/// Represents the configuration parameters for a single PDO
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PdoConfigSerializer {
    /// The COB ID this PDO will use to send/receive
    pub cob_id: u32,
    /// The COB ID for this PDO is an extended 29-bit ID
    #[serde(default)]
    pub extended: bool,
    /// Add the NODE ID to the `cob` value to get the actual COB ID
    /// The PDO is active
    pub enabled: bool,
    /// When set, this PDO will be respond to RTR requests
    #[serde(default)]
    pub rtr_disabled: bool,
    /// List of mapping specifying what sub objects are mapped to this PDO
    pub mappings: Vec<PdoMapping>,
    /// Specifies when a PDO is sent or latched
    ///
    /// - 0: Sent in response to sync, but only after an application specific event (e.g. it may be
    ///   sent when the value changes, but not when it has not)
    /// - 1 - 240: Sent in response to every Nth sync
    /// - 254: Event driven (application to send it whenever it wants)
    pub transmission_type: u8,
}

/// Represents the configuration parameters for a single PDO
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(try_from = "PdoConfigSerializer")]
pub struct PdoConfig {
    /// The COB ID this PDO will use to send/receive
    pub cob_id: CanId,
    /// Indicates if this PDO is enabled
    pub enabled: bool,
    /// If set, this PDO will not respond to requests
    pub rtr_disabled: bool,
    /// List of mapping specifying what sub objects are mapped to this PDO
    pub mappings: Vec<PdoMapping>,
    /// Specifies when a PDO is sent or latched
    ///
    /// - 0: Sent in response to sync, but only after an application specific event (e.g. it may be
    ///   sent when the value changes, but not when it has not)
    /// - 1 - 240: Sent in response to every Nth sync
    /// - 254: Event driven (application to send it whenever it wants)
    pub transmission_type: u8,
}

/// Error when deserializing a [`PdoConfigSerializer`]
#[derive(Clone, Debug, Snafu)]
#[snafu(display("{message}"))]
struct PdoConfigParseError {
    message: String,
}

impl TryFrom<PdoConfigSerializer> for PdoConfig {
    type Error = PdoConfigParseError;

    fn try_from(value: PdoConfigSerializer) -> Result<Self, Self::Error> {
        let cob_id = if value.extended {
            CanId::extended(value.cob_id)
        } else {
            if value.cob_id > 0x7ff {
                return Err(PdoConfigParseError {
                    message: format!(
                        "COB ID 0x{:x} is out of range for standard ID. Set `extended` to true.",
                        value.cob_id
                    ),
                });
            }
            CanId::std(value.cob_id as u16)
        };

        Ok(PdoConfig {
            cob_id,
            enabled: value.enabled,
            mappings: value.mappings,
            rtr_disabled: value.rtr_disabled,
            transmission_type: value.transmission_type,
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoreType {
    U32,
    U16,
    U8,
    I32,
    I16,
    I8,
    F32,
    String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreSerializer {
    pub index: u16,
    pub sub: u8,
    pub value: toml::Value,
    #[serde(rename = "type")]
    pub ty: StoreType,
}

fn deserialize_store<'de, D>(deserializer: D) -> Result<Vec<Store>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_store = Vec::<StoreSerializer>::deserialize(deserializer)?;

    let store = raw_store
        .into_iter()
        .map(|raw| {
            let value = match raw.ty {
                StoreType::U32 => {
                    let value = raw.value.as_integer().ok_or(de::Error::invalid_type(
                        de::Unexpected::Str(&raw.value.to_string()),
                        &"an integer",
                    ))?;
                    Ok(StoreValue::U32(value.try_into().map_err(|_| {
                        de::Error::invalid_value(
                            de::Unexpected::Signed(value),
                            &"an integer in range [0..2^32]",
                        )
                    })?))
                }
                StoreType::U16 => {
                    let value = raw.value.as_integer().ok_or(de::Error::invalid_type(
                        de::Unexpected::Str(&raw.value.to_string()),
                        &"an integer",
                    ))?;
                    Ok(StoreValue::U16(value.try_into().map_err(|_| {
                        de::Error::invalid_value(
                            de::Unexpected::Signed(value),
                            &"an integer in range [0..65536]",
                        )
                    })?))
                }
                StoreType::U8 => {
                    let value = raw.value.as_integer().ok_or(de::Error::invalid_type(
                        de::Unexpected::Str(&raw.value.to_string()),
                        &"an integer",
                    ))?;
                    Ok(StoreValue::U8(value.try_into().map_err(|_| {
                        de::Error::invalid_value(
                            de::Unexpected::Signed(value),
                            &"an integer in range [0..256]",
                        )
                    })?))
                }
                StoreType::I32 => {
                    let value = raw.value.as_integer().ok_or(de::Error::invalid_type(
                        de::Unexpected::Str(&raw.value.to_string()),
                        &"an integer",
                    ))?;
                    Ok(StoreValue::I32(value.try_into().map_err(|_| {
                        de::Error::invalid_value(
                            de::Unexpected::Signed(value),
                            &"an integer in range [-2^31..2^31]",
                        )
                    })?))
                }
                StoreType::I16 => {
                    let value = raw.value.as_integer().ok_or(de::Error::invalid_type(
                        de::Unexpected::Str(&raw.value.to_string()),
                        &"an integer",
                    ))?;
                    Ok(StoreValue::I16(value.try_into().map_err(|_| {
                        de::Error::invalid_value(
                            de::Unexpected::Signed(value),
                            &"an integer in range [-32767..32768]",
                        )
                    })?))
                }
                StoreType::I8 => {
                    let value = raw.value.as_integer().ok_or(de::Error::invalid_type(
                        de::Unexpected::Str(&raw.value.to_string()),
                        &"an integer",
                    ))?;
                    Ok(StoreValue::I8(value.try_into().map_err(|_| {
                        de::Error::invalid_value(
                            de::Unexpected::Signed(value),
                            &"an integer in range [-127..128]",
                        )
                    })?))
                }
                StoreType::F32 => {
                    let value = raw.value.as_float().ok_or(de::Error::invalid_type(
                        de::Unexpected::Str(&raw.value.to_string()),
                        &"a float",
                    ))?;
                    Ok(StoreValue::F32(value as f32))
                }
                StoreType::String => {
                    let value = raw.value.as_str().ok_or(de::Error::invalid_type(
                        de::Unexpected::Str(&raw.value.to_string()),
                        &"a string",
                    ))?;
                    Ok(StoreValue::String(value.to_string()))
                }
            }?;
            Ok(Store {
                index: raw.index,
                sub: raw.sub,
                value,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(store)
}

pub(crate) fn deserialize_pdo_map<'de, D, T>(deserializer: D) -> Result<HashMap<usize, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let str_map = HashMap::<String, T>::deserialize(deserializer)?;
    let original_len = str_map.len();
    let data = {
        str_map
            .into_iter()
            .map(|(str_key, value)| match str_key.parse() {
                Ok(int_key) => Ok((int_key, value)),
                Err(_) => Err({
                    de::Error::invalid_value(
                        de::Unexpected::Str(&str_key),
                        &"a non-negative integer",
                    )
                }),
            })
            .collect::<Result<HashMap<_, _>, _>>()?
    };
    // multiple strings could parse to the same int, e.g "0" and "00"
    if data.len() < original_len {
        return Err(de::Error::custom("detected duplicate integer key"));
    }
    Ok(data)
}

#[cfg(test)]
mod test {
    use super::*;
    use assertables::assert_contains;

    #[test]
    fn test_out_of_range_standard_id() {
        let str = r#"
        [tpdo.0]
        enabled = true
        cob_id = 0x800
        transmission_type = 254
        mappings = [
            { index=0x1000, sub=1, size=8 },
        ]
        "#;

        let result = NodeConfig::load_from_str(str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_contains!(
            &err.to_string(),
            "COB ID 0x800 is out of range for standard ID"
        );
    }

    #[test]
    fn test_extended_cob() {
        let str = r#"
        [tpdo.0]
        enabled = true
        cob_id = 0x800
        extended = true
        transmission_type = 254
        mappings = [
            { index=0x1000, sub=1, size=8 },
        ]
        "#;

        let result = NodeConfig::load_from_str(str).unwrap();
        assert_eq!(1, result.tpdos().len());
        let tpdo = result.tpdos().get(&0).unwrap();
        assert_eq!(CanId::extended(0x800), tpdo.cob_id);
    }

    #[test]
    fn test_node_config_parse() {
        let str = r#"
        [tpdo.0]
        enabled = true
        cob_id = 0x181
        transmission_type = 254
        mappings = [
            { index=0x1000, sub=1, size=8 },
            { index=0x1000, sub=2, size=16 },
        ]

        [[store]]
        type = "u32"
        value = 12
        index = 0x1000
        sub = 0
        "#;

        let config = match NodeConfig::load_from_str(str) {
            Ok(config) => config,
            Err(e) => {
                println!("{}", e);
                panic!("Failed to parse config");
            }
        };

        println!("{config:?}");
        assert_eq!(1, config.tpdos().len());
        assert_eq!(1, config.stores().len());
    }

    #[test]
    fn test_out_of_range_integer() {
        let str = r#"
        [[store]]
        type = "u8"
        value = 256
        index = 0x1000
        sub = 0
        "#;

        let result = NodeConfig::load_from_str(str);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected an integer in range [0..256]"));
    }
}
