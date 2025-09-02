//! A REPL-style interactive shell for talking to CAN devices via socketcan
use std::{
    array::TryFromSliceError,
    borrow::Cow,
    ffi::OsString,
    marker::PhantomData,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use clap::Parser;
use reedline::{
    default_emacs_keybindings, Emacs, FileBackedHistory, KeyModifiers, MenuBuilder, Prompt,
    PromptHistorySearch, PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    Span,
};
use shlex::Shlex;
use zencan_cli::command::{Cli, Commands, LssCommands, NmtAction, SdoDataType};
use zencan_client::{
    common::{lss::LssState, traits::AsyncCanSender, NodeId},
    open_socketcan, BusManager, NodeConfig,
};

#[derive(Parser)]
struct Args {
    /// Execute a single command
    #[arg(short, long)]
    command: Option<String>,
    /// The CAN socket to connect to (e.g. 'can0' or 'vcan0')
    socket: String,
}

struct ZencanPrompt {
    socket: String,
    node_state: Arc<Mutex<usize>>,
}

impl ZencanPrompt {
    pub fn new<S: Into<String>>(socket: S, node_state: Arc<Mutex<usize>>) -> Self {
        let socket = socket.into();
        Self { socket, node_state }
    }
}

impl Prompt for ZencanPrompt {
    fn render_prompt_left(&self) -> std::borrow::Cow<str> {
        Cow::from(&self.socket)
    }

    fn render_prompt_right(&self) -> std::borrow::Cow<str> {
        let node_state = self.node_state.lock().unwrap();
        Cow::Owned(format!("Nodes: {}", node_state))
    }

    fn render_prompt_indicator(
        &self,
        _prompt_mode: reedline::PromptEditMode,
    ) -> std::borrow::Cow<str> {
        Cow::Borrowed(">")
    }

    fn render_prompt_multiline_indicator(&self) -> std::borrow::Cow<str> {
        Cow::Borrowed("::: ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> std::borrow::Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}

struct Completer<C: Parser + Send + Sync + 'static> {
    c_phantom: PhantomData<C>,
}
impl<C: Parser + Send + Sync + 'static> Completer<C> {
    pub fn new() -> Self {
        Self {
            c_phantom: PhantomData::<C>,
        }
    }
}

impl<C: Parser + Send + Sync + 'static> reedline::Completer for Completer<C> {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<reedline::Suggestion> {
        let mut cmd = C::command();
        //let mut cmd = clap_complete::engine::complete()::CompleteCommand::augment_subcommands(cmd);

        let args = Shlex::new(line);
        let mut args = std::iter::once("".to_owned())
            .chain(args)
            .map(OsString::from)
            .collect::<Vec<_>>();
        if line.ends_with(' ') {
            args.push(OsString::new());
        }

        let arg_index = args.len() - 1;
        let span = Span::new(pos - args[arg_index].len(), pos);

        if line.is_empty() {
            return cmd
                .get_subcommands()
                .map(|cmd| reedline::Suggestion {
                    value: cmd.get_name().to_owned(),
                    description: cmd.get_after_help().map(|x| x.to_string()),
                    style: None,
                    extra: None,
                    span,
                    append_whitespace: true,
                })
                .collect();
        }
        let Ok(candidates) = clap_complete::engine::complete(
            &mut cmd,
            args,
            arg_index,
            PathBuf::from_str(".").ok().as_deref(),
        ) else {
            return vec![];
        };
        candidates
            .into_iter()
            .map(|c| reedline::Suggestion {
                value: c.get_value().to_string_lossy().into_owned(),
                description: c.get_help().map(|x| x.to_string()),
                style: None,
                extra: None,
                span,
                append_whitespace: false,
            })
            .collect()
    }
}

pub struct MismatchedSizeError {}
impl From<TryFromSliceError> for MismatchedSizeError {
    fn from(_value: TryFromSliceError) -> Self {
        Self {}
    }
}

fn convert_write_value_to_bytes(data_type: SdoDataType, value: &str) -> Result<Vec<u8>, String> {
    match data_type {
        SdoDataType::U32 => {
            let num = clap_num::maybe_hex::<u32>(value)?;
            Ok(num.to_le_bytes().to_vec())
        }
        SdoDataType::U16 => {
            let num = clap_num::maybe_hex::<u16>(value)?;
            Ok(num.to_le_bytes().to_vec())
        }
        SdoDataType::U8 => {
            let num = clap_num::maybe_hex::<u8>(value)?;
            Ok(vec![num])
        }
        SdoDataType::I32 => {
            let num = value.parse::<i32>().map_err(|e| e.to_string())?;
            Ok(num.to_le_bytes().to_vec())
        }
        SdoDataType::I16 => {
            let num = value.parse::<i16>().map_err(|e| e.to_string())?;
            Ok(num.to_le_bytes().to_vec())
        }
        SdoDataType::I8 => {
            let num = value.parse::<i8>().map_err(|e| e.to_string())?;
            Ok(vec![num as u8])
        }
        SdoDataType::F32 => {
            let num = value.parse::<f32>().map_err(|e| e.to_string())?;
            Ok(num.to_le_bytes().to_vec())
        }
        SdoDataType::Utf8 => Ok(value.as_bytes().to_vec()),
    }
}

/// Attempt to print a byte slice based on data type and return true if successful
fn convert_read_bytes_to_string(
    data_type: SdoDataType,
    bytes: &[u8],
) -> Result<String, MismatchedSizeError> {
    match data_type {
        SdoDataType::U32 => Ok(u32::from_le_bytes(bytes.try_into()?).to_string()),
        SdoDataType::U16 => Ok(u16::from_le_bytes(bytes.try_into()?).to_string()),
        SdoDataType::U8 => {
            if !bytes.is_empty() {
                Ok(bytes[0].to_string())
            } else {
                Err(MismatchedSizeError {})
            }
        }
        SdoDataType::I32 => Ok(i32::from_le_bytes(bytes.try_into()?).to_string()),
        SdoDataType::I16 => Ok(i16::from_le_bytes(bytes.try_into()?).to_string()),
        SdoDataType::I8 => {
            if !bytes.is_empty() {
                Ok((bytes[0] as i8).to_string())
            } else {
                Err(MismatchedSizeError {})
            }
        }
        SdoDataType::F32 => Ok(f32::from_le_bytes(bytes.try_into()?).to_string()),
        SdoDataType::Utf8 => Ok(String::from_utf8_lossy(bytes).to_string()),
    }
}

async fn run_command<S: AsyncCanSender + Sync + Send>(cmd: Commands, manager: &mut BusManager<S>) {
    match cmd {
        Commands::Scan => {
            let nodes = manager.scan_nodes().await;
            for n in &nodes {
                println!("{n}");
            }
        }
        Commands::Info => {
            let nodes = manager.node_list().await;
            for n in &nodes {
                println!("{n}");
            }
        }
        Commands::Nmt(cmd) => match cmd.action {
            NmtAction::ResetApp => manager.nmt_reset_app(cmd.node.raw()).await,
            NmtAction::ResetComms => manager.nmt_reset_comms(cmd.node.raw()).await,
            NmtAction::Start => manager.nmt_start(cmd.node.raw()).await,
            NmtAction::Stop => manager.nmt_stop(cmd.node.raw()).await,
        },
        Commands::LoadConfig(args) => {
            let config = match NodeConfig::load_from_file(&args.path) {
                Ok(c) => c,
                Err(e) => {
                    println!("Error reading config file: ");
                    println!("{e}");
                    return;
                }
            };
            let mut client = manager.sdo_client(args.node_id);
            for (pdo_num, cfg) in config.tpdos() {
                if let Err(e) = client.configure_tpdo(*pdo_num, cfg).await {
                    println!("Error configuring TPDO {pdo_num}:");
                    println!("{e}");
                    continue;
                }
            }
            for (pdo_num, cfg) in config.rpdos() {
                if let Err(e) = client.configure_rpdo(*pdo_num, cfg).await {
                    println!("Error configuring RPDO {pdo_num}:");
                    println!("{e}");
                    continue;
                }
            }
            for store in config.stores() {
                if let Err(e) = client
                    .download(store.index, store.sub, &store.raw_value())
                    .await
                {
                    println!(
                        "Error storing object at index {:04X} sub {}: {e}",
                        store.index, store.sub
                    );
                    continue;
                }
            }
        }
        Commands::Lss(lss_cmd) => match lss_cmd {
            LssCommands::Activate { identity } => {
                match manager.lss_activate((identity).into()).await {
                    Ok(_) => println!("Success!"),
                    Err(e) => println!("Error: {e}"),
                }
            }
            LssCommands::Fastscan { timeout } => {
                let timeout = Duration::from_millis(timeout);
                let ids = manager.lss_fastscan(timeout).await;
                println!("Found {} unconfigured nodes", ids.len());
                for id in ids {
                    println!(
                        "0x{:x} 0x{:x} 0x{:x} 0x{:x}",
                        id.vendor_id, id.product_code, id.revision, id.serial
                    );
                }
            }
            LssCommands::SetNodeId { node_id, identity } => {
                let node_id = match NodeId::try_from(node_id) {
                    Ok(id) => id,
                    Err(_) => {
                        println!("Invalid node_id {node_id}");
                        return;
                    }
                };

                if let Some(ident) = identity {
                    match manager.lss_activate(ident.into()).await {
                        Ok(_) => (),
                        Err(e) => {
                            println!("Error activating node: {e}");
                            return;
                        }
                    }
                }
                match manager.lss_set_node_id(node_id).await {
                    Ok(_) => {
                        println!("Success!");
                    }
                    Err(e) => {
                        println!("Error setting node id: {e}");
                    }
                }
            }
            LssCommands::StoreConfig { identity } => {
                if let Some(ident) = identity {
                    match manager.lss_activate(ident.into()).await {
                        Ok(_) => println!(
                            "Activated device 0x{:x} 0x{:x} 0x{:x} 0x{:x}",
                            ident.vendor_id, ident.product_code, ident.revision, ident.serial
                        ),
                        Err(e) => {
                            println!("Error activating node: {e}");
                            return;
                        }
                    }
                }
                match manager.lss_store_config().await {
                    Ok(_) => println!("Success!"),
                    Err(e) => println!("Error storing config: {e}"),
                }
            }
            LssCommands::Global { enable } => {
                let mode = if enable == 0 {
                    LssState::Waiting
                } else {
                    LssState::Configuring
                };
                manager.lss_set_global_mode(mode).await;
                println!("Commanding global {mode:?}");
            }
        },
        Commands::Read(args) => {
            // Make sure node ID is valid
            let node_id = match NodeId::new(args.node_id) {
                Ok(id) => id,
                Err(_) => {
                    println!("{} is not a valid node ID", args.node_id);
                    return;
                }
            };
            let mut client = manager.sdo_client(node_id.raw());
            match client.upload(args.index, args.sub).await {
                Ok(bytes) => match args.data_type {
                    Some(data_type) => match convert_read_bytes_to_string(data_type, &bytes) {
                        Ok(str) => {
                            println!("Value: {str}");
                        }
                        Err(_) => {
                            println!(
                                "Read invalid data size {} for type {:?}",
                                bytes.len(),
                                data_type
                            );
                            println!("Bytes: {:?}", &bytes);
                        }
                    },
                    None => {
                        println!("Read bytes: {:?}", &bytes);
                    }
                },
                Err(e) => {
                    println!("Error reading object: {e}");
                    return;
                }
            }
        }
        Commands::Write(args) => {
            // Make sure node ID is valid
            let node_id = match NodeId::new(args.node_id) {
                Ok(id) => id,
                Err(_) => {
                    println!("{} is not a valid node ID", args.node_id);
                    return;
                }
            };
            let mut client = manager.sdo_client(node_id.raw());
            match convert_write_value_to_bytes(args.data_type, &args.value) {
                Ok(bytes) => match client.download(args.index, args.sub, &bytes).await {
                    Ok(_) => {
                        println!("Wrote {} bytes", bytes.len());
                    }
                    Err(e) => {
                        println!("Download error: {e}");
                    }
                },
                Err(e) => {
                    println!("Cannot convert value to {:?}: {}", args.data_type, e);
                }
            }
        }
        Commands::SaveObjects(args) => {
            // Make sure node ID is valid
            let node_id = match NodeId::new(args.node_id) {
                Ok(id) => id,
                Err(_) => {
                    println!("{} is not a valid node ID", args.node_id);
                    return;
                }
            };
            let mut client = manager.sdo_client(node_id.raw());
            match client.save_objects().await {
                Ok(_) => println!("Node {} save succeeded", node_id.raw()),
                Err(e) => println!("Error: {e}"),
            }
        }
    }
}

fn parse_command(line: &str) -> Result<Cli, clap::Error> {
    match shlex::split(&line) {
        Some(split) => {
            Cli::try_parse_from(std::iter::once("").chain(split.iter().map(String::as_str)))
        }
        None => {
            panic!("shlex!");
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();

    let node_state = Arc::new(Mutex::new(0));
    let prompt = ZencanPrompt::new(&args.socket, node_state.clone());

    let (tx, rx) = open_socketcan(&args.socket).expect("Failed to open bus socket");
    let mut manager = BusManager::new(tx, rx);

    if let Some(cmd_string) = args.command {
        match parse_command(&cmd_string) {
            Ok(cmd) => run_command(cmd.command, &mut manager).await,
            Err(e) => {
                println!("{e}");
            }
        }
        return;
    }

    let completion_menu = Box::new(
        reedline::IdeMenu::default()
            .with_default_border()
            .with_name("completion_menu"),
    );
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        reedline::KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
    let edit_mode = Box::new(Emacs::new(keybindings));

    let mut rl = Reedline::create()
        .with_completer(Box::new(Completer::<Cli>::new()))
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_history(Box::new(
            FileBackedHistory::with_file(10000, "/tmp/zencan-cli-history".into()).unwrap(),
        ))
        .with_edit_mode(edit_mode);

    loop {
        let nodes = manager.node_list().await;
        *node_state.lock().unwrap() = nodes.len();
        let line = match rl.read_line(&prompt) {
            Ok(Signal::Success(line)) => line,
            Ok(Signal::CtrlC) => continue,
            Ok(Signal::CtrlD) => {
                println!("Exiting...");
                break;
            }
            Err(e) => panic!("Reedline error: {e}"),
        };

        match parse_command(&line) {
            Ok(cmd) => run_command(cmd.command, &mut manager).await,
            Err(e) => println!("{e}"),
        }
    }
}
