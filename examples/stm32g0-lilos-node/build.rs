fn main() {
    if let Err(e) =
        zencan_build::build_node_from_device_config("ZENCAN_CONFIG", "zencan_config.toml")
    {
        eprintln!("Failed to parse zencan_config.toml: {}", e.to_string());
        std::process::exit(-1);
    }
}
