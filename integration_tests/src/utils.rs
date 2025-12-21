#![allow(dead_code)]
use std::{future::Future, time::Instant};

use crate::sim_bus::{SimBus, SimBusReceiver, SimBusSender};
use zencan_client::SdoClient;
use zencan_common::{messages::ZencanMessage, traits::AsyncCanReceiver};
use zencan_node::Node;

pub fn get_sdo_client<'a>(
    bus: &mut SimBus<'a>,
    node_id: u8,
) -> SdoClient<SimBusSender<'a>, SimBusReceiver> {
    let sender = bus.new_sender();
    let receiver = bus.new_receiver();
    SdoClient::new_std(node_id, sender, receiver)
}

pub struct TestContext {
    channel_rx: tokio::sync::mpsc::Receiver<()>,
}

impl TestContext {
    /// Wait for node process to be called n times
    pub async fn wait_for_process(&mut self, n: usize) {
        // Flush the channel
        while self.channel_rx.try_recv().is_ok() {}
        // Wait for n cycle notices
        for _ in 0..n {
            self.channel_rx.recv().await;
        }
    }
}

#[allow(dead_code)]
pub async fn test_with_background_process<F, T, Fut>(
    nodes: &mut [&mut Node<'_>],
    bus: &mut SimBus<'_>,
    test_task: F,
) -> T
where
    F: (FnOnce(TestContext) -> Fut) + 'static,
    Fut: Future<Output = T>,
{
    // Call process once, to make sure the node is initialized before SDO requests come in
    for node in nodes.iter_mut() {
        node.process(0);
    }

    let (tx, rx) = tokio::sync::mpsc::channel(10);

    let epoch = Instant::now();
    let node_process_task = async move {
        loop {
            let now_us = Instant::now().duration_since(epoch).as_micros() as u64;
            tokio::time::sleep(tokio::time::Duration::from_micros(50)).await;
            for node in nodes.iter_mut() {
                node.process(now_us);
                // Service tx mailboxes
                bus.flush_mailboxes();
                // Send notice to the TestContext that the process cycle has been executed
                tx.try_send(()).ok();
            }
        }
    };

    let ctx = TestContext { channel_rx: rx };
    tokio::select! {
        _ = node_process_task => panic!("Node process task exited"),
        test_result = test_task(ctx) => test_result
    }
}

pub struct BusLogger {
    rx: SimBusReceiver,
}

impl BusLogger {
    #[allow(dead_code)]
    pub fn new(rx: SimBusReceiver) -> Self {
        Self { rx }
    }

    pub fn print(&mut self) {
        println!("Bus message history");
        println!("-------------------");
        while let Some(msg) = self.rx.try_recv() {
            let parsed_msg: Result<ZencanMessage, _> = msg.try_into();

            if let Ok(msg) = parsed_msg {
                println!("{:?}", msg);
            } else {
                println!("{:?}", msg);
            }
        }
    }
}

impl Drop for BusLogger {
    fn drop(&mut self) {
        self.print();
    }
}
