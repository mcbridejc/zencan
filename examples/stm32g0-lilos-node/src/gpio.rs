//! GPIO Driver

#![allow(dead_code)]
#![macro_use]

use crate::pac;
use crate::pac::gpio::{self, vals};

/// A pin object which stores info about the GPIO is controls
pub struct DynamicPin {
    pin_port: u8,
}

/// Pull setting for an input.
#[derive(Debug, Eq, PartialEq, Copy, Clone, defmt::Format)]
pub enum Pull {
    /// No pull
    None,
    /// Pull up
    Up,
    /// Pull down
    Down,
}

impl From<Pull> for vals::Pupdr {
    fn from(pull: Pull) -> Self {
        use Pull::*;

        match pull {
            None => vals::Pupdr::FLOATING,
            Up => vals::Pupdr::PULLUP,
            Down => vals::Pupdr::PULLDOWN,
        }
    }
}

/// Speed settings
///
/// These vary dpeending on the chip, ceck the reference manual or datasheet for details.
#[allow(missing_docs)]
#[derive(Debug, Copy, Clone, defmt::Format)]
pub enum Speed {
    Low,
    Medium,
    High,
    VeryHigh,
}

impl From<Speed> for vals::Ospeedr {
    fn from(speed: Speed) -> Self {
        use Speed::*;

        match speed {
            Low => vals::Ospeedr::LOWSPEED,
            Medium => vals::Ospeedr::MEDIUMSPEED,
            High => vals::Ospeedr::HIGHSPEED,
            VeryHigh => vals::Ospeedr::VERYHIGHSPEED,
        }
    }
}

impl PinPort for DynamicPin {
    fn pin_port(&self) -> u8 {
        self.pin_port
    }
}

impl Pin for DynamicPin {
    fn is_out_low(&self) -> bool {
        todo!()
    }
}

/// Digital input or output level.
#[derive(Debug, Eq, PartialEq, Copy, Clone, defmt::Format)]
pub enum Level {
    /// Low
    Low,
    /// High
    High,
}

impl From<bool> for Level {
    fn from(val: bool) -> Self {
        match val {
            true => Self::High,
            false => Self::Low,
        }
    }
}

impl From<Level> for bool {
    fn from(level: Level) -> bool {
        match level {
            Level::Low => false,
            Level::High => true,
        }
    }
}

/// GPIO output type
pub enum OutputType {
    /// Drive the pin both high or low.
    PushPull,
    /// Drive the pin low, or don't drive it at all if the output level is high.
    OpenDrain,
}

/// Alternate function type settings
#[derive(Debug, Copy, Clone, defmt::Format)]
pub enum AFType {
    /// Input
    Input,
    /// Output, drive the pin both high or low.
    OutputPushPull,
    /// Output, drive the pin low, or don't drive it at all if the output level is high.
    OutputOpenDrain,
}

#[allow(dead_code)]
pub trait Pin: Into<DynamicPin> + PinPort + Sized + 'static {
    #[inline]
    fn _pin(&self) -> u8 {
        self.pin_port() % 16
    }
    #[inline]
    fn _port(&self) -> u8 {
        self.pin_port() / 16
    }

    #[inline]
    fn block(&self) -> gpio::Gpio {
        pac::GPIO(self._port() as _)
    }

    /// Set the output as high.
    #[inline]
    fn set_high(&self) {
        let n = self._pin() as _;
        self.block().bsrr().write(|w| w.set_bs(n, true));
    }

    /// Set the output as low.
    #[inline]
    fn set_low(&self) {
        let n = self._pin() as _;
        self.block().bsrr().write(|w| w.set_br(n, true));
    }

    #[inline]
    fn set_as_af(&self, af_num: u8, af_type: AFType) {
        self.set_as_af_pull(af_num, af_type, Pull::None);
    }

    #[inline]
    fn set_as_af_pull(&self, af_num: u8, af_type: AFType, pull: Pull) {
        let pin = self._pin() as usize;
        let block = self.block();
        block.afr(pin / 8).modify(|w| w.set_afr(pin % 8, af_num));
        match af_type {
            AFType::Input => {}
            AFType::OutputPushPull => block.otyper().modify(|w| w.set_ot(pin, vals::Ot::PUSHPULL)),
            AFType::OutputOpenDrain => block
                .otyper()
                .modify(|w| w.set_ot(pin, vals::Ot::OPENDRAIN)),
        }
        block.pupdr().modify(|w| w.set_pupdr(pin, pull.into()));

        block
            .moder()
            .modify(|w| w.set_moder(pin, vals::Moder::ALTERNATE));
    }

    #[inline]
    fn set_as_analog(&self) {
        let pin = self._pin() as usize;
        let block = self.block();
        block
            .moder()
            .modify(|w| w.set_moder(pin, vals::Moder::ANALOG));
    }

    /// Set the pin as "disconnected", ie doing nothing and consuming the lowest
    /// amount of power possible.
    ///
    /// This is currently the same as set_as_analog but is semantically different really.
    /// Drivers should set_as_disconnected pins when dropped.
    #[inline]
    fn set_as_disconnected(&self) {
        self.set_as_analog();
    }

    #[inline]
    fn set_speed(&self, speed: Speed) {
        let pin = self._pin() as usize;

        self.block()
            .ospeedr()
            .modify(|w| w.set_ospeedr(pin, speed.into()));
    }

    /// Number of the pin within the port (0..31)
    #[inline]
    fn pin(&self) -> u8 {
        self._pin()
    }

    /// Port of the pin
    #[inline]
    fn port(&self) -> u8 {
        self._port()
    }

    fn is_low(&self) -> bool {
        let state = self.block().idr().read().idr(self.pin() as _);
        state == vals::Idr::LOW
    }

    #[inline]
    fn is_high(&self) -> bool {
        !self.is_low()
    }

    #[inline]
    fn is_out_high(&self) -> bool {
        !self.is_out_low()
    }

    #[inline]
    fn is_out_low(&self) -> bool {
        let state = self.block().odr().read().odr(self.pin() as _);
        state == vals::Odr::LOW
    }

    /// Put the pin into output mode.
    ///
    /// The pin level will be whatever was set before (or low by default). If you want it to begin
    /// at a specific level, call `set_high`/`set_low` on the pin first.
    #[inline]
    fn set_as_output(&mut self, speed: Speed) {
        critical_section::with(|_| {
            let r = self.block();
            let n = self.pin() as usize;

            r.pupdr().modify(|w| w.set_pupdr(n, vals::Pupdr::FLOATING));
            r.otyper().modify(|w| w.set_ot(n, vals::Ot::PUSHPULL));
            self.set_speed(speed);
            r.moder().modify(|w| w.set_moder(n, vals::Moder::OUTPUT));
        });
    }
}

pub struct Gpio<const PIN_PORT: u8> {}

pub type PA0 = Gpio<0>;
pub type PA1 = Gpio<1>;
pub type PA2 = Gpio<2>;
pub type PA3 = Gpio<3>;
pub type PA4 = Gpio<4>;
pub type PA5 = Gpio<5>;
pub type PA6 = Gpio<6>;
pub type PA7 = Gpio<7>;
pub type PA8 = Gpio<8>;
pub type PA9 = Gpio<9>;
pub type PA10 = Gpio<10>;
pub type PA11 = Gpio<11>;
pub type PA12 = Gpio<12>;
pub type PA13 = Gpio<13>;
pub type PA14 = Gpio<14>;
pub type PA15 = Gpio<15>;

pub type PB0 = Gpio<{ (1 << 4) + 0 }>;
pub type PB1 = Gpio<{ (1 << 4) + 1 }>;
pub type PB2 = Gpio<{ (1 << 4) + 2 }>;
pub type PB3 = Gpio<{ (1 << 4) + 3 }>;
pub type PB4 = Gpio<{ (1 << 4) + 4 }>;
pub type PB5 = Gpio<{ (1 << 4) + 5 }>;
pub type PB6 = Gpio<{ (1 << 4) + 6 }>;
pub type PB7 = Gpio<{ (1 << 4) + 7 }>;
pub type PB8 = Gpio<{ (1 << 4) + 8 }>;
pub type PB9 = Gpio<{ (1 << 4) + 9 }>;
pub type PB10 = Gpio<{ (1 << 4) + 10 }>;
pub type PB11 = Gpio<{ (1 << 4) + 11 }>;
pub type PB12 = Gpio<{ (1 << 4) + 12 }>;
pub type PB13 = Gpio<{ (1 << 4) + 13 }>;
pub type PB14 = Gpio<{ (1 << 4) + 14 }>;
pub type PB15 = Gpio<{ (1 << 4) + 15 }>;

pub type PC0 = Gpio<{ (2 << 4) + 0 }>;
pub type PC1 = Gpio<{ (2 << 4) + 1 }>;
pub type PC2 = Gpio<{ (2 << 4) + 2 }>;
pub type PC3 = Gpio<{ (2 << 4) + 3 }>;
pub type PC4 = Gpio<{ (2 << 4) + 4 }>;
pub type PC5 = Gpio<{ (2 << 4) + 5 }>;
pub type PC6 = Gpio<{ (2 << 4) + 6 }>;
pub type PC7 = Gpio<{ (2 << 4) + 7 }>;
pub type PC8 = Gpio<{ (2 << 4) + 8 }>;
pub type PC9 = Gpio<{ (2 << 4) + 9 }>;
pub type PC10 = Gpio<{ (2 << 4) + 10 }>;
pub type PC11 = Gpio<{ (2 << 4) + 11 }>;
pub type PC12 = Gpio<{ (2 << 4) + 12 }>;
pub type PC13 = Gpio<{ (2 << 4) + 13 }>;
pub type PC14 = Gpio<{ (2 << 4) + 14 }>;
pub type PC15 = Gpio<{ (2 << 4) + 15 }>;

pub type PD0 = Gpio<{ (3 << 4) + 0 }>;
pub type PD1 = Gpio<{ (3 << 4) + 1 }>;
pub type PD2 = Gpio<{ (3 << 4) + 2 }>;
pub type PD3 = Gpio<{ (3 << 4) + 3 }>;
pub type PD4 = Gpio<{ (3 << 4) + 4 }>;
pub type PD5 = Gpio<{ (3 << 4) + 5 }>;
pub type PD6 = Gpio<{ (3 << 4) + 6 }>;
pub type PD7 = Gpio<{ (3 << 4) + 7 }>;
pub type PD8 = Gpio<{ (3 << 4) + 8 }>;
pub type PD9 = Gpio<{ (3 << 4) + 9 }>;
pub type PD10 = Gpio<{ (3 << 4) + 10 }>;
pub type PD11 = Gpio<{ (3 << 4) + 11 }>;
pub type PD12 = Gpio<{ (3 << 4) + 12 }>;
pub type PD13 = Gpio<{ (3 << 4) + 13 }>;
pub type PD14 = Gpio<{ (3 << 4) + 14 }>;
pub type PD15 = Gpio<{ (3 << 4) + 15 }>;

pub trait PinPort {
    fn pin_port(&self) -> u8;
}

impl<const PIN_PORT: u8> PinPort for Gpio<PIN_PORT> {
    fn pin_port(&self) -> u8 {
        PIN_PORT
    }
}

impl<const PIN_PORT: u8> From<Gpio<PIN_PORT>> for DynamicPin {
    fn from(value: Gpio<PIN_PORT>) -> Self {
        DynamicPin {
            pin_port: value.pin_port(),
        }
    }
}

impl<const PIN_PORT: u8> Pin for Gpio<PIN_PORT> {}

#[allow(non_snake_case, dead_code)]
pub struct Gpios {
    pub PA0: PA0,
    pub PA1: PA1,
    pub PA2: PA2,
    pub PA3: PA3,
    pub PA4: PA4,
    pub PA5: PA5,
    pub PA6: PA6,
    pub PA7: PA7,
    pub PA8: PA8,
    pub PA9: PA9,
    pub PA10: PA10,
    pub PA11: PA11,
    pub PA12: PA12,
    pub PA13: PA13,
    pub PA14: PA14,
    pub PA15: PA15,

    pub PB0: PB0,
    pub PB1: PB1,
    pub PB2: PB2,
    pub PB3: PB3,
    pub PB4: PB4,
    pub PB5: PB5,
    pub PB6: PB6,
    pub PB7: PB7,
    pub PB8: PB8,
    pub PB9: PB9,
    pub PB10: PB10,
    pub PB11: PB11,
    pub PB12: PB12,
    pub PB13: PB13,
    pub PB14: PB14,
    pub PB15: PB15,

    pub PC0: PC0,
    pub PC1: PC1,
    pub PC2: PC2,
    pub PC3: PC3,
    pub PC4: PC4,
    pub PC5: PC5,
    pub PC6: PC6,
    pub PC7: PC7,
    pub PC8: PC8,
    pub PC9: PC9,
    pub PC10: PC10,
    pub PC11: PC11,
    pub PC12: PC12,
    pub PC13: PC13,
    pub PC14: PC14,
    pub PC15: PC15,

    pub PD0: PD0,
    pub PD1: PD1,
    pub PD2: PD2,
    pub PD3: PD3,
    pub PD4: PD4,
    pub PD5: PD5,
    pub PD6: PD6,
    pub PD7: PD7,
    pub PD8: PD8,
    pub PD9: PD9,
    pub PD10: PD10,
    pub PD11: PD11,
    pub PD12: PD12,
    pub PD13: PD13,
    pub PD14: PD14,
    pub PD15: PD15,
}

pub fn gpios() -> Gpios {
    Gpios {
        PA0: PA0 {},
        PA1: PA1 {},
        PA2: PA2 {},
        PA3: PA3 {},
        PA4: PA4 {},
        PA5: PA5 {},
        PA6: PA6 {},
        PA7: PA7 {},
        PA8: PA8 {},
        PA9: PA9 {},
        PA10: PA10 {},
        PA11: PA11 {},
        PA12: PA12 {},
        PA13: PA13 {},
        PA14: PA14 {},
        PA15: PA15 {},

        PB0: PB0 {},
        PB1: PB1 {},
        PB2: PB2 {},
        PB3: PB3 {},
        PB4: PB4 {},
        PB5: PB5 {},
        PB6: PB6 {},
        PB7: PB7 {},
        PB8: PB8 {},
        PB9: PB9 {},
        PB10: PB10 {},
        PB11: PB11 {},
        PB12: PB12 {},
        PB13: PB13 {},
        PB14: PB14 {},
        PB15: PB15 {},

        PC0: PC0 {},
        PC1: PC1 {},
        PC2: PC2 {},
        PC3: PC3 {},
        PC4: PC4 {},
        PC5: PC5 {},
        PC6: PC6 {},
        PC7: PC7 {},
        PC8: PC8 {},
        PC9: PC9 {},
        PC10: PC10 {},
        PC11: PC11 {},
        PC12: PC12 {},
        PC13: PC13 {},
        PC14: PC14 {},
        PC15: PC15 {},

        PD0: PD0 {},
        PD1: PD1 {},
        PD2: PD2 {},
        PD3: PD3 {},
        PD4: PD4 {},
        PD5: PD5 {},
        PD6: PD6 {},
        PD7: PD7 {},
        PD8: PD8 {},
        PD9: PD9 {},
        PD10: PD10 {},
        PD11: PD11 {},
        PD12: PD12 {},
        PD13: PD13 {},
        PD14: PD14 {},
        PD15: PD15 {},
    }
}
