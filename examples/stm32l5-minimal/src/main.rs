#![no_std]
#![no_main]

use cortex_m::peripheral::syst::SystClkSource;
use cortex_m_rt::{entry, exception};
use panic_halt as _;
use portable_atomic::AtomicU64;
use stm32_metapac as pac;
use zencan_node::{Callbacks, Node, common::NodeId};
mod can;

mod zencan {
    zencan_node::include_modules!(ZENCAN_CONFIG);
}

static ELAPSED_MS: AtomicU64 = AtomicU64::new(0);

#[entry]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().unwrap();

    // Use the internal 16 MHz HSI as the PLL input. PLL1 is configured for 8 MHz on both its R
    // (system clock) and Q (FDCAN kernel clock) outputs.
    pac::RCC.cr().modify(|w| {
        w.set_hsion(true);
    });
    while !pac::RCC.cr().read().hsirdy() {}

    pac::RCC.pllcfgr().modify(|w| {
        w.set_pllsrc(pac::rcc::vals::Pllsrc::HSI);
        w.set_pllm(pac::rcc::vals::Pllm::DIV2);
        w.set_plln(pac::rcc::vals::Plln::MUL8);
        w.set_pllq(pac::rcc::vals::Pllq::DIV8);
        w.set_pllqen(true);
        w.set_pllr(pac::rcc::vals::Pllr::DIV8);
        w.set_pllren(true);
    });
    pac::RCC.cr().modify(|w| w.set_pllon(true));
    while !pac::RCC.cr().read().pllrdy() {}

    pac::RCC
        .cfgr()
        .modify(|w| w.set_sw(pac::rcc::vals::Sw::PLL1_R));
    while pac::RCC.cfgr().read().sws() != pac::rcc::vals::Sw::PLL1_R {}

    pac::RCC.ahb2enr().modify(|w| w.set_gpioben(true));
    pac::RCC.apb1enr2().modify(|w| w.set_fdcan1en(true));
    pac::RCC
        .ccipr()
        .modify(|w| w.set_fdcansel(pac::rcc::vals::Fdcansel::PLL1_Q));

    pac::GPIOB.moder().modify(|w| {
        w.set_moder(7, pac::gpio::vals::Moder::OUTPUT);
        w.set_moder(8, pac::gpio::vals::Moder::ALTERNATE);
        w.set_moder(9, pac::gpio::vals::Moder::ALTERNATE);
    });
    pac::GPIOB.afr(1).modify(|w| {
        w.set_afr(0, 9);
        w.set_afr(1, 9);
    });
    pac::GPIOB.bsrr().write(|w| w.set_br(7, true));
    can::init();

    let callbacks = Callbacks::default();
    let od = zencan::get_od();
    let mut node = Node::new(
        NodeId::new(1).unwrap(),
        callbacks,
        od.node_mbox(),
        od.node_state(),
        od,
    );
    od.node_mbox()
        .set_transmit_notify_callback(&can::transmit_notify_handler);

    const CPU_HZ: u32 = 8_000_000;
    const TICK_HZ: u32 = 1_000;
    cp.SYST.set_reload(CPU_HZ / TICK_HZ - 1);
    cp.SYST.clear_current();
    cp.SYST.set_clock_source(SystClkSource::Core);
    cp.SYST.enable_interrupt();
    cp.SYST.enable_counter();
    unsafe { cortex_m::interrupt::enable() };

    loop {
        let elapsed = ELAPSED_MS.load(core::sync::atomic::Ordering::Relaxed);
        node.process(elapsed * 1000);
    }
}

#[exception]
fn SysTick() {
    ELAPSED_MS.add(1, core::sync::atomic::Ordering::Relaxed);
}
