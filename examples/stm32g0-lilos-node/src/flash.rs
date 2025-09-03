//! Flash driver for STM32G0 family

#![allow(dead_code)]

use core::{
    ptr::slice_from_raw_parts,
    sync::atomic::{Ordering, fence},
};

use crate::{
    pac::flash::Flash,
    persist::{FlashAccess, Page},
};

const PAGE_SIZE: usize = 2048;

pub struct FlashError {}

pub struct Stm32g0Flash {
    flash: Flash,

    page_a: usize,
    page_b: usize,
}

pub struct Stm32g0FlashUnlocked<'a> {
    flash: &'a Flash,

    cache: [u8; 8],
    active_page: Page,

    page_a: usize,
    page_b: usize,

    write_pos: usize,
}

impl Stm32g0Flash {
    pub fn new(flash: Flash, page_a: usize, page_b: usize) -> Self {
        Self {
            flash,
            page_a,
            page_b,
        }
    }

    fn page_slice(&self, n: usize) -> &[u8] {
        let addr = (crate::pac::FLASH_BASE + n * PAGE_SIZE) as *const u8;
        // Safety: I don't think the flash is going anywhere
        unsafe { slice_from_raw_parts(addr, PAGE_SIZE).as_ref().unwrap() }
    }

    pub fn page(&self, page: Page) -> &[u8] {
        match page {
            Page::A => self.page_slice(self.page_a),
            Page::B => self.page_slice(self.page_b),
        }
    }

    pub fn unlock<'a>(&'a mut self) -> Stm32g0FlashUnlocked<'a> {
        self.flash.keyr().write_value(0x45670123);
        self.flash.keyr().write_value(0xCDEF89AB);
        Stm32g0FlashUnlocked {
            flash: &self.flash,
            cache: [0; 8],
            active_page: Page::A,
            page_a: self.page_a,
            page_b: self.page_b,
            write_pos: 0,
        }
    }
}

impl<'a> Stm32g0FlashUnlocked<'a> {
    pub fn lock(self) {
        self.flash.cr().modify(|w| w.set_lock(true));
    }
}

impl<'a> Drop for Stm32g0FlashUnlocked<'a> {
    fn drop(&mut self) {
        self.flash.cr().modify(|w| w.set_lock(true));
    }
}

impl<'a> Stm32g0FlashUnlocked<'a> {
    fn page_slice(&self, n: usize) -> &[u8] {
        let addr = (crate::pac::FLASH_BASE + n * PAGE_SIZE) as *const u8;
        // Safety: I don't think the flash is going anywhere
        unsafe { slice_from_raw_parts(addr, PAGE_SIZE).as_ref().unwrap() }
    }

    fn page_ptr_mut(&mut self, n: usize, offset: usize) -> *mut u32 {
        (crate::pac::FLASH_BASE + n * PAGE_SIZE + offset) as *mut u32
    }

    fn active_page_num(&self) -> usize {
        match self.active_page {
            Page::A => self.page_a,
            Page::B => self.page_b,
        }
    }

    fn write_cache(&mut self, offset: usize) {
        let word1 = u32::from_le_bytes(self.cache[0..4].try_into().unwrap());
        let word2 = u32::from_le_bytes(self.cache[4..8].try_into().unwrap());

        let dst1 = self.page_ptr_mut(self.active_page_num(), offset);
        let dst2 = self.page_ptr_mut(self.active_page_num(), offset + 4);
        self.clear_errors();
        self.wait_busy();

        self.flash.cr().modify(|w| w.set_pg(true));

        // Writing to flash must be done as a sequence of two 32-bit writes, starting on a 64-bit
        // aligned address
        fence(Ordering::SeqCst);
        unsafe { core::ptr::write_volatile(dst1, word1) };
        fence(Ordering::SeqCst);
        unsafe { core::ptr::write_volatile(dst2, word2) };
        fence(Ordering::SeqCst);
        self.wait_busy();

        self.flash.sr().write(|w| w.set_eop(true));
        self.flash.cr().modify(|w| w.set_pg(false));
    }

    fn clear_errors(&mut self) -> u32 {
        let sr = self.flash.sr().read().0;
        // Clear error flags
        self.flash.sr().modify(|w| {
            w.set_fasterr(true);
            w.set_miserr(true);
            w.set_operr(true);
            w.set_pgserr(true);
            w.set_pgaerr(true);
            w.set_progerr(true);
            w.set_rderr(true);
            w.set_sizerr(true);
            w.set_wrperr(true);
        });

        sr
    }

    fn wait_busy(&mut self) {
        // note: bsy() here is bsy1, bit 16
        while self.flash.sr().read().bsy() {}
    }
}

impl<'a> FlashAccess for Stm32g0FlashUnlocked<'a> {
    type Error = FlashError;

    fn page(&self, page: Page) -> &[u8] {
        match page {
            Page::A => self.page_slice(self.page_a),
            Page::B => self.page_slice(self.page_b),
        }
    }

    fn set_write_page(&mut self, page: Page) {
        self.active_page = page;
        self.write_pos = 0;
    }

    fn erase(&mut self) {
        self.wait_busy();
        self.clear_errors();

        self.flash.cr().modify(|w| {
            w.set_per(true);
            w.set_pnb(self.active_page_num() as u8);
        });
        self.flash.cr().modify(|w| {
            w.set_strt(true);
        });

        self.wait_busy();

        self.flash.cr().modify(|w| w.set_per(false));
    }

    fn write(&mut self, data: &[u8]) {
        // Data has to be written in 64-bit chunks, aligned to 64-bit words.

        let mut in_pos = 0;

        while in_pos < data.len() {
            let buf_pos = self.write_pos % 8;
            let to_copy = (8 - buf_pos).min(data.len() - in_pos);
            self.cache[buf_pos..buf_pos + to_copy].copy_from_slice(&data[in_pos..in_pos + to_copy]);
            in_pos += to_copy;
            self.write_pos += to_copy;
            if self.write_pos % 8 == 0 {
                self.write_cache(self.write_pos - 8);
            }
        }
    }

    fn finalize(&mut self) {
        // Pad remaining bytes with 0s
        let buf_pos = self.write_pos % 8;
        if buf_pos == 0 {
            return;
        }
        self.cache[buf_pos..8].fill(0);
        self.write_cache(self.write_pos & !0x7);
    }
}
