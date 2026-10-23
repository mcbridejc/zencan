use zencan_build::device_config::DeviceConfig;

const BASE: &str = r#"
    device_name = "test"

    [identity]
    vendor_id = 1234
    product_code = 12000
    revision_number = 1
"#;

/// The generated SDO_BUFFER_SIZE definition, with whitespace removed
fn sdo_buffer_size(objects: &str) -> String {
    let config = DeviceConfig::load_from_str(&format!("{BASE}\n{objects}"))
        .expect("Failed to parse device config");
    let code: String = zencan_build::device_config_to_string(&config, false)
        .expect("Failed to compile")
        .split_whitespace()
        .collect();
    let start = code
        .find("constSDO_BUFFER_SIZE:usize=")
        .expect("No SDO_BUFFER_SIZE in generated code");
    let end = start + code[start..].find(';').unwrap();
    code[start..end].to_string()
}

#[test]
fn sdo_buffer_is_sized_to_the_largest_object() {
    let size = sdo_buffer_size(
        r#"
        [[objects]]
        index = 0x2000
        parameter_name = "Name"
        object_type = "var"
        data_type = "VisibleString(40)"
        access_type = "rw"
        "#,
    );
    assert_eq!(
        size,
        "constSDO_BUFFER_SIZE:usize=zencan_node::sdo_buffer_size(40usize)"
    );
}

#[test]
fn sdo_buffer_is_full_size_with_a_domain() {
    let size = sdo_buffer_size(
        r#"
        [[objects]]
        index = 0x2000
        parameter_name = "Domain"
        object_type = "var"
        data_type = "domain"
        access_type = "rw"
        "#,
    );
    assert_eq!(
        size,
        "constSDO_BUFFER_SIZE:usize=zencan_node::SDO_BUFFER_SIZE"
    );
}
