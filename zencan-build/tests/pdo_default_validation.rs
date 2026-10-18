use assertables::assert_contains;
use zencan_build::{
    device_config::{DeviceConfig, FrameFormat},
    device_config_to_string,
};

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

/// A device whose TPDO 0 maps `mappings`, with the given `[can]` section and `rtr_disabled` value
fn wide_config(can_section: &str, rtr_disabled: bool, mappings: &str) -> DeviceConfig {
    DeviceConfig::load_from_str(&format!(
        r#"
        device_name = "wide"
        {can_section}

        [identity]
        vendor_id = 1
        product_code = 2
        revision_number = 3

        [pdos.tpdo.0]
        enabled = true
        cob_id = 0x180
        add_node_id = true
        rtr_disabled = {rtr_disabled}
        transmission_type = 254
        mappings = [{mappings}]

        [[objects]]
        index = 0x2000
        parameter_name = "Wide values"
        object_type = "array"
        data_type = "uint64"
        access_type = "ro"
        array_size = 8
        default_value = [0, 0, 0, 0, 0, 0, 0, 0]
        pdo_mapping = "tpdo"

        [[objects]]
        index = 0x2001
        parameter_name = "Block"
        object_type = "var"
        data_type = "octetstring(24)"
        access_type = "ro"
        pdo_mapping = "tpdo"
        "#
    ))
    .unwrap()
}

const FD: &str = "[can]\nframe_format = \"fd\"";
const CLASSIC: &str = "[can]\nframe_format = \"classic\"";

/// Eight 64-bit mappings: 64 bytes
const MAPPINGS_64_BYTES: &str = "{ index = 0x2000, sub = 1, size = 64 }, \
    { index = 0x2000, sub = 2, size = 64 }, { index = 0x2000, sub = 3, size = 64 }, \
    { index = 0x2000, sub = 4, size = 64 }, { index = 0x2000, sub = 5, size = 64 }, \
    { index = 0x2000, sub = 6, size = 64 }, { index = 0x2000, sub = 7, size = 64 }, \
    { index = 0x2000, sub = 8, size = 64 }";

#[test]
fn frame_format_defaults_to_classic() {
    assert_eq!(FrameFormat::Classic, example().can.frame_format);
    let config = wide_config("", false, "{ index = 0x2000, sub = 1, size = 64 }");
    assert_eq!(FrameFormat::Classic, config.can.frame_format);
    let config = wide_config(CLASSIC, false, "{ index = 0x2000, sub = 1, size = 64 }");
    assert_eq!(FrameFormat::Classic, config.can.frame_format);
    let config = wide_config(FD, true, "{ index = 0x2000, sub = 1, size = 64 }");
    assert_eq!(FrameFormat::Fd, config.can.frame_format);

    let invalid = DeviceConfig::load_from_str(
        "device_name = \"x\"\n[can]\nframe_format = \"xl\"\n[identity]\nvendor_id = 1\nproduct_code = 2\nrevision_number = 3\n",
    );
    assert!(invalid.is_err());
}

/// Generated code with all whitespace removed, to match regardless of formatting
fn generated_code(config: &DeviceConfig) -> String {
    let code = device_config_to_string(config, false).unwrap();
    code.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn classic_generates_classic_frame_type() {
    let code = generated_code(&example());
    assert_contains!(code, "pubtypeFrame=zencan_node::common::can::CanMessage;");
    assert_contains!(code, "!<FrameasCanFrame>::IS_FD&&");
    assert_contains!(code, "<FrameasCanFrame>::MAX_LEN>=8usize");
}

#[test]
fn fd_accepts_up_to_64_bytes() {
    let code = generated_code(&wide_config(FD, true, MAPPINGS_64_BYTES));
    assert_contains!(code, "pubtypeFrame=zencan_node::common::can::CanFdMessage;");
    assert_contains!(code, "assert!(<FrameasCanFrame>::IS_FD&&");
    assert_contains!(code, "<FrameasCanFrame>::MAX_LEN>=64usize");
    assert_contains!(code, "staticNODE_MBOX:StaticStorage<NodeMbox<Frame>>");
}

#[test]
fn classic_rejects_more_than_8_bytes() {
    // Two 64-bit mappings fit a CAN FD frame, but not a classic one
    let mappings = "{ index = 0x2000, sub = 1, size = 64 }, { index = 0x2000, sub = 2, size = 8 }";
    let result = device_config_to_string(&wide_config(CLASSIC, false, mappings), false);
    assert_contains!(
        result.unwrap_err().to_string(),
        "mappings exceed one CAN frame"
    );
    let result = device_config_to_string(&wide_config(CLASSIC, false, MAPPINGS_64_BYTES), false);
    assert_contains!(
        result.unwrap_err().to_string(),
        "mappings exceed one CAN frame"
    );
    assert!(device_config_to_string(&wide_config(FD, true, mappings), false).is_ok());
}

#[test]
fn fd_rejects_more_than_64_bytes() {
    // Three 24-byte mappings: 72 bytes
    let mappings = "{ index = 0x2001, sub = 0, size = 192 }, \
        { index = 0x2001, sub = 0, size = 192 }, { index = 0x2001, sub = 0, size = 192 }";
    let result = device_config_to_string(&wide_config(FD, true, mappings), false);
    assert_contains!(
        result.unwrap_err().to_string(),
        "mappings exceed one CAN FD frame (64 bytes)"
    );
}

#[test]
fn fd_rejects_more_than_8_mappings() {
    let mappings = ["{ index = 0x2000, sub = 1, size = 8 }"; 9].join(", ");
    let result = device_config_to_string(&wide_config(FD, true, &mappings), false);
    assert_contains!(result.unwrap_err().to_string(), "more than 8 mappings");
}

#[test]
fn fd_rejects_rtr() {
    let mappings = "{ index = 0x2000, sub = 1, size = 64 }";
    let result = device_config_to_string(&wide_config(FD, false, mappings), false);
    assert_contains!(
        result.unwrap_err().to_string(),
        "CAN FD has no remote frames, so rtr_disabled must be set to true"
    );
    // Classic CAN keeps accepting RTR
    assert!(device_config_to_string(&wide_config(CLASSIC, false, mappings), false).is_ok());
}
