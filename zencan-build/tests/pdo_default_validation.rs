use assertables::assert_contains;
use zencan_build::{device_config::DeviceConfig, device_config_to_string};

fn example() -> DeviceConfig {
    DeviceConfig::load_from_str(include_str!(
        "../../integration_tests/device_configs/example1.toml"
    ))
    .unwrap()
}

#[test]
fn rejects_unrepresentable_or_incompatible_pdo_defaults() {
    // add_node_id is true, so 0x7ff + node_id can result in a value too high for a standard ID
    let mut config = example();
    config.pdos.tpdo_defaults.get_mut(&1).unwrap().cob_id = 0x7ff;
    assert!(device_config_to_string(&config, false).is_err());
    // Change a mapping to an invalid object
    let mut config = example();
    config.pdos.tpdo_defaults.get_mut(&1).unwrap().mappings[0].index = 0xffff;
    let result = device_config_to_string(&config, false);
    assert!(result.is_err());
    assert_contains!(
        result.unwrap_err().to_string(),
        "mapped object does not exist"
    );
    // Set too many mappings
    let mut config = example();
    let defaults = config.pdos.tpdo_defaults.get_mut(&1).unwrap();
    defaults.mappings = vec![defaults.mappings[0]; 9];
    let result = device_config_to_string(&config, false);
    assert!(result.is_err());
    assert_contains!(
        result.unwrap_err().to_string(),
        "mappings exceed one CAN frame"
    );
    // Only multiples of 8-bit sizes are allowed
    let mut config = example();
    config.pdos.tpdo_defaults.get_mut(&1).unwrap().mappings[0].size = 7;
    let result = device_config_to_string(&config, false);
    assert!(result.is_err());
    assert_contains!(
        result.unwrap_err().to_string(),
        "incompatible mapping type, width or access"
    );
    // Cannot set defaults for a non-existent PDO
    let mut config = example();
    let defaults = config.pdos.tpdo_defaults.remove(&1).unwrap();
    config.pdos.tpdo_defaults.insert(4, defaults);
    let result = device_config_to_string(&config, false);
    assert!(result.is_err());
    assert_contains!(
        result.unwrap_err().to_string(),
        "slot exceeds configured PDO count"
    );
}
