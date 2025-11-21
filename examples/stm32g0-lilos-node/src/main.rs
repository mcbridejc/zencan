//! Example project to create a zencan node and read ADC channels to objects
//!
#![no_std]
#![no_main]

use core::{
    cell::RefCell, convert::Infallible, num::{NonZeroU16, NonZeroU8}, pin::pin, time::Duration
};

use core::hash::Hasher;
use hash32::{FnvHasher, Hasher as _};

use lilos::{exec::Notify, time::Millis};
use persist::SectionUpdate;
use stm32_metapac::{self as pac, RCC, interrupt};

use fdcan::{
    FdCan, FdCanControl, Fifo0, NormalOperationMode, Rx,
    config::{DataBitTiming, FdCanConfig, GlobalFilter},
    filter::{StandardFilter, StandardFilterSlot},
};

use cortex_m_rt as _;
use panic_probe as _;
use rtt_target::{self as _, rtt_init, set_defmt_channel};

use zencan_node::{
    common::NodeId, object_dict::{ODEntry, ObjectAccess}, restore_stored_comm_objects, restore_stored_objects, Callbacks, Node
};

/// Create a serial number from the UID register
fn get_serial() -> u32 {
    let mut hasher: FnvHasher = Default::default();
    hasher.write_u32(pac::UID.uid(0).read());
    hasher.write_u32(pac::UID.uid(1).read());
    hasher.write_u32(pac::UID.uid(2).read());
    hasher.finish32()
}

mod adc;
mod flash;
mod gpio;
mod persist;
mod zencan {
    zencan_node::include_modules!(ZENCAN_CONFIG);
}

use adc::{configure_adc, read_adc};
use flash::Stm32g0Flash;
use gpio::Pin;
use zencan::{OBJECT2000, OBJECT2001, OBJECT2002};

struct FdCan1 {}
unsafe impl fdcan::message_ram::Instance for FdCan1 {
    const MSG_RAM: *mut fdcan::message_ram::RegisterBlock = pac::FDCANRAM1.as_ptr() as _;
}
unsafe impl fdcan::Instance for FdCan1 {
    const REGISTERS: *mut fdcan::RegisterBlock = pac::FDCAN1.as_ptr() as _;
}

static mut CAN_RX: Option<Rx<FdCan1, NormalOperationMode, Fifo0>> = None;
static mut CAN_CTRL: Option<FdCanControl<FdCan1, NormalOperationMode>> = None;
static CAN_NOTIFY: Notify = Notify::new();

enum FlashSections {
    NodeConfig = 1,
    Objects = 2,
    Unknown = 256,
}

impl From<u8> for FlashSections {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::NodeConfig,
            2 => Self::Objects,
            _ => Self::Unknown,
        }
    }
}

/// Callback from zencan to store object data to flash
#[allow(static_mut_refs)]
fn store_objects(flash: &mut Stm32g0Flash, reader: &mut dyn embedded_io::Read<Error = Infallible>, size: usize) {
    if persist::update_sections(
        &mut flash.unlock(),
        &mut [SectionUpdate {
            section_id: FlashSections::Objects as u8,
            data: persist::UpdateSource::Reader((reader, size)),
        }],
    )
    .is_err()
    {
        defmt::error!("Error storing objects to flash");
    }
}

/// Callback from zencan to store node configuraiton to flash
#[allow(static_mut_refs)]
fn store_node_config(flash: &mut Stm32g0Flash, id: NodeId) {
    let data = [id.raw()];
    if persist::update_sections(
        &mut flash.unlock(),
        &mut [SectionUpdate {
            section_id: FlashSections::NodeConfig as u8,
            data: persist::UpdateSource::Slice(&data),
        }],
    )
    .is_err()
    {
        defmt::error!("Error storing node config to flash");
    }
}

/// Callback to notify CAN task that there are messages to be processed
fn notify_can_task() {
    CAN_NOTIFY.notify();
}

/// Read the node ID from flash
fn read_saved_node_id(flash: &mut Stm32g0Flash) -> NodeId {
    if let Some(sections) = persist::load_sections(&flash.unlock()) {
        for s in sections {
            let section_type = FlashSections::from(s.section_id);
            match section_type {
                FlashSections::NodeConfig => {
                    if s.data.len() > 0 {
                        match NodeId::try_from(s.data[0]) {
                            Ok(node_id) => return node_id,
                            Err(_) => {
                                defmt::error!("Read invalid node_id {} from flash", s.data[0]);
                                break;
                            }
                        }
                    } else {
                        defmt::error!("Found zero length NodeConfig section");
                    }
                }
                _ => continue,
            }
        }
    }

    NodeId::Unconfigured
}

fn read_persisted_objects(flash: &mut Stm32g0Flash, restore_fn: impl Fn(&[u8])) {
    if let Some(sections) = persist::load_sections(&flash.unlock()) {
        for s in sections {
            let section_type = FlashSections::from(s.section_id);
            match section_type {
                FlashSections::NodeConfig => (), // Ignore
                FlashSections::Objects => {
                    defmt::info!("Loaded objects from flash");
                    restore_fn(s.data);
                }
                FlashSections::Unknown => {
                    defmt::warn!("Found unrecognized flash section {}", s.section_id);
                }
            }
        }
    } else {
        defmt::info!("No data found in flash");
    }
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().unwrap();

    pac::FLASH
        .acr()
        .modify(|w| w.set_latency(pac::flash::vals::Latency::WS0));

    RCC.cfgr().modify(|w| {
        w.set_sw(pac::rcc::vals::Sw::HSI);
        w.set_hpre(pac::rcc::vals::Hpre::DIV1);
    });

    RCC.apbenr1().modify(|w| {
        w.set_fdcanen(true);
    });

    // Enable clock to all the GPIO ports
    RCC.gpioenr().modify(|w| {
        w.set_gpioaen(true);
        w.set_gpioben(true);
        w.set_gpiocen(true);
        w.set_gpioden(true);
        w.set_gpiofen(true);
    });

    let channels = rtt_init! {
        up: {
            0: {
                size: 512,
                name: "defmt",
            }
        }
    };
    set_defmt_channel(channels.up.0);

    // The last two pages of flash are set aside for non-volatile storage
    // Each page is 2kB
    const FLASH_PAGE_A: usize = 62;
    const FLASH_PAGE_B: usize = 63;
    let mut flash = flash::Stm32g0Flash::new(pac::FLASH, FLASH_PAGE_A, FLASH_PAGE_B);

    let gpios = gpio::gpios();

    // Setup CAN peripheral pins to the appropriate alternate function
    let can_rx_pin = gpios.PB8;
    let mut can_tx_pin = gpios.PB9;
    can_tx_pin.set_high();
    can_tx_pin.set_as_output(gpio::Speed::High);
    can_rx_pin.set_as_af(3, gpio::AFType::Input);
    can_tx_pin.set_as_af(3, gpio::AFType::OutputPushPull);

    // Initialize the FDCAN peripheral
    let mut can = FdCan::new(FdCan1 {}).into_config_mode();
    // Bit timing calculated at http://www.bittiming.can-wiki.info/
    let can_config = FdCanConfig::default()
        .set_automatic_retransmit(false)
        .set_frame_transmit(fdcan::config::FrameTransmissionConfig::ClassicCanOnly)
        .set_data_bit_timing(DataBitTiming {
            transceiver_delay_compensation: false,
            prescaler: NonZeroU8::new(1).unwrap(),
            seg1: NonZeroU8::new(13).unwrap(),
            seg2: NonZeroU8::new(2).unwrap(),
            sync_jump_width: NonZeroU8::new(1).unwrap(),
        })
        .set_nominal_bit_timing(fdcan::config::NominalBitTiming {
            prescaler: NonZeroU16::new(1).unwrap(),
            seg1: NonZeroU8::new(13).unwrap(),
            seg2: NonZeroU8::new(2).unwrap(),
            sync_jump_width: NonZeroU8::new(1).unwrap(),
        })
        .set_global_filter(GlobalFilter {
            handle_standard_frames: fdcan::config::NonMatchingFilter::IntoRxFifo0,
            handle_extended_frames: fdcan::config::NonMatchingFilter::IntoRxFifo0,
            reject_remote_standard_frames: false,
            reject_remote_extended_frames: false,
        });

    can.apply_config(can_config);
    let mut can = can.into_normal();

    can.enable_interrupt(fdcan::interrupt::Interrupt::RxFifo0NewMsg);
    can.enable_interrupt_line(fdcan::config::InterruptLine::_1, true);
    can.set_standard_filter(
        StandardFilterSlot::_0,
        StandardFilter::accept_all_into_fifo0(),
    );

    let (can_ctrl, mut can_tx, can_rx0, _can_rx1) = can.split();

    // Store the rx and ctrl handles to statics for the ISR to use
    unsafe {
        CAN_RX = Some(can_rx0);
        CAN_CTRL = Some(can_ctrl);
    }

    configure_adc();

    let node_id = read_saved_node_id(&mut flash);

    // Use the UID register to set a unique serial number
    zencan::OBJECT1018.set_serial(get_serial());

    let flash = RefCell::new(flash);

    let mut store_node_config = |node_id| { store_node_config(&mut flash.borrow_mut(), node_id); };
    let mut store_objects = |reader: &mut dyn embedded_io::Read<Error = Infallible>, len| { store_objects(&mut flash.borrow_mut(), reader, len) };
    let mut reset_app = |od: &[ODEntry]| {
        // On RESET APP transition, we reload object values to their reset value

        // Init defaults for application objects. In a future release, objects should provide a
        // better API for resetting defaults, but for now, it can be done here by the application if
        // desired.
        for i in 0..4 {
            zencan::OBJECT2000.set(i, 0).ok();
            zencan::OBJECT2001.set(i, 0).ok();
            zencan::OBJECT2002.set(i, 0).ok();
            zencan::OBJECT2200.set(i, 1).ok();
            zencan::OBJECT2201.set(i, 1).ok();
            zencan::OBJECT2202.set(i, 0).ok();
        }
        zencan::OBJECT2100.set_value(20);

        // Restore objects saved to flash
        read_persisted_objects(&mut flash.borrow_mut(), |stored_data| restore_stored_objects(od, stored_data));
    };
    let mut reset_comms = |od: &[ODEntry]| {
        // On reset COMMS, only the communications objects (0x1000-0x1fff) are restored. The node
        // library will handle restoring the default values before calling the reset_comms callback.
        // Then the application may restore objects from persistent storage if it supports that.
        read_persisted_objects(&mut flash.borrow_mut(), |stored_data| restore_stored_comm_objects(od, stored_data));
    };


    let callbacks = Callbacks {
        send_message: &mut move |msg| {
            let header = zencan_to_fdcan_header(&msg);
            if let Err(_) = can_tx.transmit(header, msg.data()) {
                defmt::error!("Error transmitting CAN message");
            }
            Ok(())
        },
        store_node_config: Some(&mut store_node_config),
        store_objects: Some(&mut store_objects),
        reset_app: Some(&mut reset_app),
        reset_comms: Some(&mut reset_comms),
        enter_operational: None,
        enter_stopped: None,
        enter_preoperational: None,
    };

    let node = Node::new(
        node_id,
        callbacks,
        &zencan::NODE_MBOX,
        &zencan::NODE_STATE,
        &zencan::OD_TABLE,
    );

    // Register handler for waking process task
    zencan::NODE_MBOX.set_process_notify_callback(&notify_can_task);

    // Enable debugger access while sleeping
    pac::DBGMCU.cr().modify(|w| {
        w.set_dbg_standby(true);
        w.set_dbg_stop(true);
    });
    // Enabling the DMA keeps the clock on to enable debugger memory access during sleep, which is
    // needed for RTT access
    pac::RCC.ahbenr().modify(|w| w.set_dma1en(true));

    // Set up the OS timer.
    lilos::time::initialize_sys_tick(&mut cp.SYST, 16_000_000);

    unsafe { cortex_m::interrupt::enable() };
    unsafe { cortex_m::peripheral::NVIC::unmask(pac::Interrupt::TIM16_FDCAN_IT0) };

    lilos::exec::run_tasks(
        &mut [pin!(can_task(node)), pin!(main_task())],
        lilos::exec::ALL_TASKS,
    )
}

/// Create an fdcan TxFrameHeader from a zencan CanMessage
fn zencan_to_fdcan_header(msg: &zencan_node::common::CanMessage) -> fdcan::frame::TxFrameHeader {
    let id: fdcan::id::Id = match msg.id() {
        zencan_node::common::messages::CanId::Extended(id) => {
            fdcan::id::ExtendedId::new(id).unwrap().into()
        }
        zencan_node::common::messages::CanId::Std(id) => {
            fdcan::id::StandardId::new(id).unwrap().into()
        }
    };
    fdcan::frame::TxFrameHeader {
        len: msg.dlc,
        frame_format: fdcan::frame::FrameFormat::Standard,
        id,
        bit_rate_switching: false,
        marker: None,
    }
}

/// A task for running the CAN node processing periodically, or when triggered by the CAN receive
/// interrupt to run immediately
async fn can_task(mut node: Node<'_>) -> Infallible {
    let epoch = lilos::time::TickTime::now();
    let mut timing_pin = gpio::gpios().PB5;
    timing_pin.set_as_output(gpio::Speed::High);
    loop {
        lilos::time::with_timeout(Duration::from_millis(10), CAN_NOTIFY.until_next()).await;
        timing_pin.set_high();
        let time_us = epoch.elapsed().0 * 1000;
        node.process(time_us);
        timing_pin.set_low();
    }
}

/// Task for periodically reading the sensors
async fn main_task() -> Infallible {
    const MAX_PERIOD: u32 = 5000;
    // Read the sample period from the config object, but limit the value to MAX_PERIOD
    let mut read_interval = zencan::OBJECT2100.get_value().max(MAX_PERIOD);
    let mut periodic_gate =
        lilos::time::PeriodicGate::new_shift(Millis(read_interval as u64), Millis(0));

    loop {
        periodic_gate.next_time().await;

        // Sample ADCs
        let adc_values = [read_adc(0), read_adc(1), read_adc(2), read_adc(3)];

        // Store values to raw and scaled objects
        for i in 0..4 {
            let raw_value = adc_values[i];
            OBJECT2000.set(i, adc_values[i]).unwrap();
            let scale_num = zencan::OBJECT2200.get(i).unwrap() as i32;
            let scale_den = zencan::OBJECT2201.get(i).unwrap() as i32;
            let offset = zencan::OBJECT2202.get(i).unwrap() as i32;
            let scaled_value = ((raw_value as i32 + offset).saturating_mul(scale_num)) / scale_den;

            OBJECT2001.set(i, scaled_value as i32).unwrap();
            OBJECT2002
                .set(
                    i,
                    scaled_value.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                )
                .unwrap();

            // For array objects, sub0 contains the size of the array, and the first array element
            // is stored at index 1
            let sub_idx = i as u8 + 1;

            // Set the event flags on the updated objects. When the objects are mapped to TPDOs
            // configured for async transmission, this triggers the transmission on next call to
            // process().
            OBJECT2000.set_event_flag(sub_idx).unwrap();
            OBJECT2001.set_event_flag(sub_idx).unwrap();
            OBJECT2002.set_event_flag(sub_idx).unwrap();
        }

        // Notify can task that there is something new to process
        CAN_NOTIFY.notify();

        // Check for change to period configuration
        let new_interval = zencan::OBJECT2100.get_value();
        if new_interval != read_interval {
            read_interval = new_interval;
            periodic_gate = lilos::time::PeriodicGate::new_shift(
                Millis(read_interval as u64),
                Millis(read_interval as u64),
            );
        }
    }
}

#[allow(static_mut_refs)]
#[interrupt]
fn TIM16_FDCAN_IT0() {
    // safety: Accept for during boot-up when we set it, we only access in this interrupt
    let ctrl = unsafe { CAN_CTRL.as_mut().unwrap() };
    let rx = unsafe { CAN_RX.as_mut().unwrap() };

    ctrl.clear_interrupt(fdcan::interrupt::Interrupt::RxFifo0NewMsg);

    let mut buffer = [0u8; 8];

    while let Ok(msg) = rx.receive(&mut buffer) {
        // ReceiveOverrun::unwrap() cannot fail
        let msg = msg.unwrap();

        let id = match msg.id {
            fdcan::id::Id::Standard(standard_id) => {
                zencan_node::common::messages::CanId::std(standard_id.as_raw())
            }
            fdcan::id::Id::Extended(extended_id) => {
                zencan_node::common::messages::CanId::extended(extended_id.as_raw())
            }
        };
        let msg = zencan_node::common::messages::CanMessage::new(id, &buffer[..msg.len as usize]);
        // Ignore error -- as an Err is returned for messages that are not consumed by the node
        // stack
        zencan::NODE_MBOX.store_message(msg).ok();
    }
}
