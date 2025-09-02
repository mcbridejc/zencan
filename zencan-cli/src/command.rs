use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_num::maybe_hex;
use std::{path::PathBuf, str::FromStr};
use zencan_client::common::lss::LssIdentity;

#[derive(Debug, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Read an object via SDO
    Read(ReadArgs),
    /// Write an object via SDO
    Write(WriteArgs),
    /// Scan all node IDs to find configured devices
    Scan,
    /// Scan the PDO configuration from a node
    ScanPdoConfig(ScanPdoConfigArgs),
    /// Print info about nodes
    Info,
    /// Load a configuration from a file to a node
    LoadConfig(LoadConfigArgs),
    /// Send command to save persistable objects
    SaveObjects(SaveObjectsArgs),
    /// NMT commands
    Nmt(NmtArgs),
    /// LSS commands
    #[command(subcommand)]
    Lss(LssCommands),
}

#[derive(Debug, Args)]
pub struct ReadArgs {
    /// The ID of the node to read from
    pub node_id: u8,
    /// The object index to read
    #[clap(value_parser=maybe_hex::<u16>)]
    pub index: u16,
    /// The sub object to read
    #[clap(value_parser=maybe_hex::<u8>)]
    pub sub: u8,
    /// How to interpret the response (optional)
    pub data_type: Option<SdoDataType>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum SdoDataType {
    U32,
    U16,
    U8,
    I32,
    I16,
    I8,
    F32,
    Utf8,
}

#[derive(Debug, Args)]
pub struct WriteArgs {
    /// The ID of the node to read from
    pub node_id: u8,
    /// The object index to read
    #[clap(value_parser=maybe_hex::<u16>)]
    pub index: u16,
    /// The sub object to read
    #[clap(value_parser=maybe_hex::<u8>)]
    pub sub: u8,
    /// How to interpret the value
    pub data_type: SdoDataType,
    /// The value to write
    pub value: String,
}

#[derive(Debug, Args)]
pub struct ScanPdoConfigArgs {
    pub node_id: u8,
}

#[derive(Debug, Args)]
pub struct LoadConfigArgs {
    /// The ID of the node to load the configuration into
    pub node_id: u8,
    /// Path to a node config TOML file
    #[arg(value_hint=clap::ValueHint::FilePath)]
    pub path: PathBuf,
}

#[derive(Debug, Args)]
pub struct SaveObjectsArgs {
    /// The ID of the node to command
    pub node_id: u8,
}

/// Specifies a node to apply an NMT command
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NmtNodeArg {
    All,
    Specific(u8),
}

impl NmtNodeArg {
    pub fn raw(&self) -> u8 {
        match self {
            Self::All => 0,
            Self::Specific(id) => *id,
        }
    }
}

impl FromStr for NmtNodeArg {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<u8>() {
            Ok(num) => {
                if num == 0 {
                    Ok(Self::All)
                } else if num < 128 {
                    Ok(Self::Specific(num))
                } else {
                    Err("Node ID must be between 0 and 127")
                }
            }
            Err(_) => {
                if s == "all" {
                    Ok(Self::All)
                } else {
                    Err("Must specify a node ID, or 'all' to broadcast")
                }
            }
        }
    }
}

#[derive(Debug, Args)]
pub struct NmtArgs {
    pub action: NmtAction,
    /// Specify the node ID to command. Use '0' or 'all' to broadcast to all nodes.
    pub node: NmtNodeArg,
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum NmtAction {
    ResetApp,
    ResetComms,
    Start,
    Stop,
}

#[derive(Args, Clone, Copy, Debug)]
#[group(multiple=true, requires_all=["vendor_id", "product_code", "revision", "serial"])]
pub struct IdentityArgs {
    #[clap(value_parser=maybe_hex::<u32>)]
    #[arg(required = false)]
    pub vendor_id: u32,
    /// The product to configure
    #[clap(value_parser=maybe_hex::<u32>)]
    #[arg(required = false)]
    pub product_code: u32,
    /// The revision to configure
    #[clap(value_parser=maybe_hex::<u32>)]
    #[arg(required = false)]
    pub revision: u32,
    /// The serial number
    #[clap(value_parser=maybe_hex::<u32>)]
    #[arg(required = false)]
    pub serial: u32,
}

impl From<IdentityArgs> for LssIdentity {
    fn from(value: IdentityArgs) -> Self {
        LssIdentity {
            vendor_id: value.vendor_id,
            product_code: value.product_code,
            revision: value.revision,
            serial: value.serial,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum LssCommands {
    /// Put the specified device into configuration mode, and put all others into waiting mode
    Activate {
        #[clap(flatten)]
        identity: IdentityArgs,
    },
    /// Perform a fastscan to find unconfigured nodes
    Fastscan {
        /// Timeout for waiting for fastscan response in milliseconds
        #[arg(default_value = "5")]
        timeout: u64,
    },
    SetNodeId {
        /// The node ID to assign
        node_id: u8,
        #[clap(flatten)]
        identity: Option<IdentityArgs>,
    },
    StoreConfig {
        #[clap(flatten)]
        identity: Option<IdentityArgs>,
    },
    /// Globally enable or disable configuration mode
    Global {
        /// 0 to put in waiting, 1 to put into configuration
        #[clap(action=clap::ArgAction::Set)]
        enable: u8,
    },
}
