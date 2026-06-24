//! Generate object dictionary rust code from an EDS file
//!
//!

use std::path::PathBuf;

use clap::Parser;

use zencan_build::device_config::DeviceConfig;
use zencan_build::device_config_to_string;

#[derive(Clone, Debug, Parser)]
struct Args {
    config: PathBuf,
    #[clap(short, long)]
    format: bool,
}

fn main() {
    let args = Args::parse();

    let config_content = std::fs::read_to_string(&args.config).unwrap_or_else(|_| {
        panic!(
            "Failed reading device config file {}",
            args.config.display()
        )
    });

    let config = match DeviceConfig::load_from_str(&config_content) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to parse TOML file: {}", e);
            std::process::exit(1);
        }
    };

    let compiled = device_config_to_string(&config, args.format).expect("Failed to compile");

    println!("{}", compiled);
}
