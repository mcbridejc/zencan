#![cfg_attr(not(target_os = "linux"), allow(unused_imports, dead_code))]
use clap::Parser;
use zencan_client::common::{
    can::{AsyncCanReceiver, CanMessage},
    protocol::{MessageError, ZencanMessage},
};

#[derive(Parser)]
struct Args {
    socket: String,
    #[clap(short, long)]
    verbose: bool,
    /// Open the socket in CAN FD mode, to also receive CAN FD frames
    #[clap(long)]
    fd: bool,
}

pub enum Message {
    Unrecognized {
        msg: CanMessage,
        reason: MessageError,
    },
    Recognized(ZencanMessage),
}

impl From<CanMessage> for Message {
    fn from(msg: CanMessage) -> Self {
        // Attempt to parse as a recognized Zencan message, and fallback to displaying it as a
        // generic can message
        match msg.try_into() {
            Ok(msg) => Message::Recognized(msg),
            Err(e) => Message::Unrecognized { msg, reason: e },
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("zencandump uses socketcan, so currently only works on linux.");
}

fn timestamp() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

fn print_message(time: &str, msg: CanMessage, verbose: bool) {
    match msg.into() {
        Message::Recognized(msg) => println!("{time}: {msg:?}"),
        Message::Unrecognized { msg, reason } => {
            println!("{time}: {msg:?}");
            if verbose {
                println!("Unrecognized reason: {reason:?}");
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() {
    let args = Args::parse();
    if args.fd {
        let (_tx, mut rx) = zencan_client::open_socketcan_fd(&args.socket).unwrap();
        loop {
            if let Ok(frame) = rx.recv().await {
                // Frames which fit in 8 bytes may carry CANopen protocol messages
                match CanMessage::try_from(frame) {
                    Ok(msg) => print_message(&timestamp(), msg, args.verbose),
                    Err(_) => println!("{}: {frame:?}", timestamp()),
                }
            }
        }
    }

    let (_tx, mut rx) = zencan_client::open_socketcan(&args.socket).unwrap();
    loop {
        if let Ok(msg) = rx.recv().await {
            print_message(&timestamp(), msg, args.verbose);
        }
    }
}
