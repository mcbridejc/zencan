use zencan_build::device_config::DeviceConfig;

#[test]
fn compile_test() {
    const CONFIG: &str = include_str!("example_device_config.toml");

    let config = DeviceConfig::load_from_str(CONFIG).expect("Failed to parse example config");

    let _compiled =
        zencan_build::device_config_to_string(&config, false).expect("Failed to compile");
}
