#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use embassy_executor::Spawner;
use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use embedded_can::Frame;
use embedded_can::Id::{Extended, Standard};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::efuse::Efuse;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::twai::{EspTwaiFrame, StandardId, TwaiMode, TwaiRx, TwaiTx};
use esp_hal::{twai, Async};
use esp_println::logger;
use zencan_node::Callbacks;
use zencan_node::{common::NodeId, Node};

mod zencan {
    zencan_node::include_modules!(ZENCAN_CONFIG);
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static CANOPEN_PROCESS_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static CANOPEN_TX_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    logger::init_logger(log::LevelFilter::Info);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    let tx_pin = peripherals.GPIO2;
    let rx_pin = peripherals.GPIO0;
    const TWAI_BAUDRATE: twai::BaudRate = twai::BaudRate::B125K;

    let twai_config = twai::TwaiConfiguration::new(
        peripherals.TWAI0,
        rx_pin,
        tx_pin,
        TWAI_BAUDRATE,
        TwaiMode::Normal,
    );

    let (twai_rx, twai_tx) = twai_config.into_async().start().split();

    let mac_address = Efuse::read_base_mac_address();
    log::info!("MAC address: {mac_address:?}");

    let last_mac_bytes: [u8; 4] = mac_address[2..].try_into().unwrap();
    let serial = u32::from_be_bytes(last_mac_bytes);

    zencan::OBJECT1018.set_serial(serial);
    zencan::NODE_MBOX.set_process_notify_callback(&notify_canopen_process_task);
    zencan::NODE_MBOX.set_transmit_notify_callback(&notify_canopen_tx_task);

    spawner.spawn(twai_rx_task(twai_rx)).unwrap();
    spawner.spawn(twai_tx_task(twai_tx)).unwrap();
    spawner.spawn(canopen_process_task()).unwrap();
}

fn notify_canopen_process_task() {
    CANOPEN_PROCESS_SIGNAL.signal(());
}

fn notify_canopen_tx_task() {
    CANOPEN_TX_SIGNAL.signal(());
}

#[embassy_executor::task]
async fn twai_tx_task(mut twai_tx: TwaiTx<'static, Async>) {
    loop {
        while let Some(msg) = zencan::NODE_MBOX.next_transmit_message() {
            let frame =
                EspTwaiFrame::new(StandardId::new(msg.id.raw() as u16).unwrap(), msg.data())
                    .unwrap();
            if let Err(e) = twai_tx.transmit_async(&frame).await {
                log::error!("Error sending CAN message: {e:?}");
            }
        }

        // Wait for wakeup signal when new CAN messages become ready for sending
        CANOPEN_TX_SIGNAL.wait().await;
    }
}

#[embassy_executor::task]
async fn canopen_process_task() {
    let callbacks = Callbacks {
        store_node_config: None,
        store_objects: None,
        reset_app: None,
        reset_comms: None,
        enter_operational: None,
        enter_stopped: None,
        enter_preoperational: None,
    };
    let mut node = Node::new(
        NodeId::Unconfigured,
        callbacks,
        &zencan::NODE_MBOX,
        &zencan::NODE_STATE,
        &zencan::OD_TABLE,
    );
    loop {
        select(CANOPEN_PROCESS_SIGNAL.wait(), Timer::after_millis(10)).await;
        let now_us = embassy_time::Instant::now().as_micros();

        node.process(now_us);
    }
}

#[embassy_executor::task]
async fn twai_rx_task(mut twai_rx: TwaiRx<'static, Async>) {
    loop {
        let rx_frame = twai_rx.receive_async().await.unwrap();

        let id = match rx_frame.id() {
            Standard(id) => zencan_node::common::messages::CanId::std(id.as_raw()),
            Extended(id) => zencan_node::common::messages::CanId::extended(id.as_raw()),
        };

        let msg = zencan_node::common::messages::CanMessage::new(id, rx_frame.data());
        zencan::NODE_MBOX.store_message(msg).ok();
    }
}
