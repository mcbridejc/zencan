use core::{cell::RefCell, num::NonZeroU8};

use crate::{pac, zencan};
use critical_section::Mutex;
use fdcan::{
    FdCan, NormalOperationMode,
    config::{DataBitTiming, FdCanConfig, GlobalFilter, NominalBitTiming},
    filter::{StandardFilter, StandardFilterSlot},
};
use pac::interrupt;

struct FdCan1;

unsafe impl fdcan::message_ram::Instance for FdCan1 {
    const MSG_RAM: *mut fdcan::message_ram::RegisterBlock = 0x4000_ac00 as _;
}

unsafe impl fdcan::Instance for FdCan1 {
    const REGISTERS: *mut fdcan::RegisterBlock = 0x4000_a400 as _;
}

static CAN: Mutex<RefCell<Option<FdCan<FdCan1, NormalOperationMode>>>> =
    Mutex::new(RefCell::new(None));

pub fn init() {
    let mut can = FdCan::new(FdCan1 {}).into_config_mode();
    let prescaler = NonZeroU8::new(1).unwrap();
    can.apply_config(
        FdCanConfig::default()
            .set_automatic_retransmit(false)
            .set_frame_transmit(fdcan::config::FrameTransmissionConfig::ClassicCanOnly)
            .set_data_bit_timing(DataBitTiming {
                transceiver_delay_compensation: false,
                prescaler,
                seg1: NonZeroU8::new(13).unwrap(),
                seg2: NonZeroU8::new(2).unwrap(),
                sync_jump_width: NonZeroU8::new(1).unwrap(),
            })
            .set_nominal_bit_timing(NominalBitTiming {
                prescaler: prescaler.into(),
                seg1: NonZeroU8::new(13).unwrap(),
                seg2: NonZeroU8::new(2).unwrap(),
                sync_jump_width: NonZeroU8::new(1).unwrap(),
            })
            .set_global_filter(GlobalFilter {
                handle_standard_frames: fdcan::config::NonMatchingFilter::IntoRxFifo0,
                handle_extended_frames: fdcan::config::NonMatchingFilter::IntoRxFifo0,
                reject_remote_standard_frames: false,
                reject_remote_extended_frames: false,
            }),
    );
    let mut can = can.into_normal();
    can.enable_interrupt(fdcan::interrupt::Interrupt::RxFifo0NewMsg);
    can.enable_interrupt(fdcan::interrupt::Interrupt::TxComplete);
    can.enable_interrupt_line(fdcan::config::InterruptLine::_1, true);
    can.set_standard_filter(
        StandardFilterSlot::_0,
        StandardFilter::accept_all_into_fifo0(),
    );

    pac::FDCAN1.txbtie().write(|w| w.0 = 7);

    critical_section::with(|cs| CAN.borrow_ref_mut(cs).replace(can));
    unsafe { cortex_m::peripheral::NVIC::unmask(pac::Interrupt::FDCAN1_IT0) };
}

pub fn transmit_notify_handler() {
    critical_section::with(|cs| {
        if let Some(can) = CAN.borrow_ref_mut(cs).as_mut() {
            transmit_can_messages(can);
        }
    });
}

fn transmit_can_messages(can: &mut FdCan<FdCan1, NormalOperationMode>) {
    while !pac::FDCAN1.txfqs().read().tfqf() {
        let Some(msg) = zencan::get_od().node_mbox().next_transmit_message() else {
            break;
        };
        let id = match msg.id() {
            zencan_node::common::can::CanId::Extended(id) => {
                fdcan::id::ExtendedId::new(id).unwrap().into()
            }
            zencan_node::common::can::CanId::Std(id) => {
                fdcan::id::StandardId::new(id).unwrap().into()
            }
        };
        let header = fdcan::frame::TxFrameHeader {
            len: msg.dlc,
            frame_format: fdcan::frame::FrameFormat::Standard,
            id,
            bit_rate_switching: false,
            marker: None,
        };
        let _ = can.transmit(header, msg.data());
    }
}

#[interrupt]
fn FDCAN1_IT0() {
    let cs = unsafe { critical_section::CriticalSection::new() };
    let mut cell = CAN.borrow_ref_mut(cs);
    let can = cell.as_mut().unwrap();

    if can.has_interrupt(fdcan::interrupt::Interrupt::RxFifo0NewMsg) {
        can.clear_interrupt(fdcan::interrupt::Interrupt::RxFifo0NewMsg);
        let mut buffer = [0u8; 8];
        while let Ok(frame) = can.receive0(&mut buffer) {
            let frame = frame.unwrap();
            let id = match frame.id {
                fdcan::id::Id::Standard(id) => zencan_node::common::can::CanId::std(id.as_raw()),
                fdcan::id::Id::Extended(id) => {
                    zencan_node::common::can::CanId::extended(id.as_raw())
                }
            };
            let message =
                zencan_node::common::can::CanMessage::new(id, &buffer[..frame.len as usize]);
            zencan::get_od().node_mbox().store_message(message).ok();
        }
    }

    if can.has_interrupt(fdcan::interrupt::Interrupt::TxComplete) {
        can.clear_interrupt(fdcan::interrupt::Interrupt::TxComplete);
        transmit_can_messages(can);
    }
}
