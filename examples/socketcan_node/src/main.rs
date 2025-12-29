use std::{
    convert::Infallible,
    io::Write as _,
    sync::Arc,
    time::{Duration, Instant},
};

use clap::Parser;
use tokio::time::timeout;
use zencan_node::Node;
use zencan_node::{
    common::{
        traits::{AsyncCanReceiver, AsyncCanSender},
        NodeId,
    },
    Callbacks,
};

use zencan_node::open_socketcan;

mod zencan {
    zencan_node::include_modules!(DEVICE);
}

#[derive(Parser, Debug)]
struct Args {
    socket: String,
    #[clap(long, short, default_value = "255")]
    node_id: u8,
    #[clap(long, short)]
    storage: bool,
    #[clap(long)]
    serial: Option<u32>,
}

#[tokio::main]
async fn main() {
    // Initialize the logger
    env_logger::init();
    let args = Args::parse();

    log::info!("Starting node...");
    let node_id = NodeId::try_from(args.node_id).unwrap();

    // Set the serial number using the provided serial, or a random number if none is provided
    zencan::OBJECT1018.set_serial(args.serial.unwrap_or(rand::random()));

    let object_storage_path = format!("zencan_node.{}.flash", node_id.raw());

    let mut store_objects = |reader: &mut dyn embedded_io::Read<Error = Infallible>,
                             _len: usize| {
        log::info!("Storing objects to {}", &object_storage_path);

        match std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&object_storage_path)
        {
            Ok(mut f) => {
                let mut buf = [0; 32];
                loop {
                    let n = reader.read(&mut buf).unwrap();
                    f.write_all(&buf[..n]).unwrap();
                    if n != buf.len() {
                        break;
                    }
                }
            }
            Err(e) => log::error!("Error storing objects to {}: {:?}", object_storage_path, e),
        }
    };

    let mut reset_app = |od| {
        if let Ok(data) = std::fs::read(&object_storage_path) {
            zencan_node::restore_stored_objects(od, &data);
        }
    };

    let mut reset_comms = |od| {
        if let Ok(data) = std::fs::read(&object_storage_path) {
            zencan_node::restore_stored_comm_objects(od, &data);
        }
    };

    let callbacks = Callbacks {
        store_node_config: None,
        store_objects: Some(&mut store_objects),
        reset_app: Some(&mut reset_app),
        reset_comms: Some(&mut reset_comms),
        enter_operational: None,
        enter_stopped: None,
        enter_preoperational: None,
    };

    let mut node = Node::new(
        node_id,
        callbacks,
        &zencan::NODE_MBOX,
        &zencan::NODE_STATE,
        &zencan::OD_TABLE,
    );

    let (mut tx, mut rx) = open_socketcan(&args.socket).unwrap();

    // Node requires callbacks be static, so use Box::leak to make static ref from closure on heap
    let process_notify = Box::leak(Box::new(tokio::sync::Notify::new()));
    let notify_cb = Box::leak(Box::new(|| {
        process_notify.notify_one();
    }));
    zencan::NODE_MBOX.set_process_notify_callback(notify_cb);

    // Spawn a task to receive messages
    tokio::spawn(async move {
        loop {
            let msg = match rx.recv().await {
                Ok(msg) => msg,
                Err(e) => {
                    log::error!("Error receiving message: {e:?}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            if let Err(msg) = zencan::NODE_MBOX.store_message(msg) {
                log::warn!("Unhandled RX message: {:?}", msg);
            }
        }
    });

    // Spawn a task to send messages
    tokio::spawn(async move {
        let notify = Arc::new(tokio::sync::Notify::new());
        let notify_clone = notify.clone();
        let transmit_notify_callback = Box::leak(Box::new(move || {
            notify_clone.notify_waiters();
        }));
        zencan::NODE_MBOX.set_transmit_notify_callback(transmit_notify_callback);
        loop {
            notify.notified().await;
            while let Some(msg) = zencan::NODE_MBOX.next_transmit_message() {
                if let Err(e) = tx.send(msg).await {
                    log::warn!("Error sending frame: {e:?}");
                }
            }
        }
    });

    let epoch = Instant::now();
    loop {
        let now_us = Instant::now().duration_since(epoch).as_micros() as u64;
        // Run node processing, collecting messages to send
        node.process(now_us);

        // Wait for notification to run, or a timeout
        timeout(Duration::from_millis(1), process_notify.notified())
            .await
            .ok();
    }
}
