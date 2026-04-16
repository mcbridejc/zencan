#![cfg_attr(not(target_os = "linux"), allow(unused_imports))]
use clap::Parser;
use zencan_client::common::{
    messages::{MessageError, ZencanMessage},
    traits::AsyncCanReceiver,
    CanMessage,
};

#[derive(Parser)]
struct Args {
    socket: String,
    #[clap(short, long)]
    verbose: bool,
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

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() {
    let args = Args::parse();
    let (_tx, mut rx) = zencan_client::open_socketcan(&args.socket).unwrap();

    loop {
        if let Ok(msg) = rx.recv().await {
            let time = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false);

            match msg.into() {
                Message::Recognized(msg) => println!("{time}: {msg:?}"),
                Message::Unrecognized { msg, reason } => {
                    println!("{time}: {msg:?}");
                    if args.verbose {
                        println!("Unrecognized reason: {reason:?}");
                    }
                }
            }
        }
    }
}
