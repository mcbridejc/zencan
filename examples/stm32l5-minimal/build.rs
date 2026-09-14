fn main() {
    if let Err(error) =
        zencan_build::build_node_from_device_config("ZENCAN_CONFIG", "zencan_config.toml")
    {
        eprintln!("Failed to parse zencan_config.toml: {error}");
        std::process::exit(1);
    }
}
