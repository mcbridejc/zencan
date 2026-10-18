fn main() {
    // The `fd` feature selects the CAN FD device config
    let config = if std::env::var_os("CARGO_FEATURE_FD").is_some() {
        "zencan_config_fd.toml"
    } else {
        "zencan_config.toml"
    };
    if let Err(error) = zencan_build::build_node_from_device_config("ZENCAN_CONFIG", config) {
        eprintln!("Failed to parse {config}: {error}");
        std::process::exit(1);
    }
}
