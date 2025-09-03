//! Non-volatile data persistence
//!
//! This supports writing multiple sections of data to flash using a dual-page setup, so that a new
//! page can be fully written before the old page is invalidated, ensuring no data-loss.
//!
//! The [FlashAccess] trait provides access to two pages of storage, and must be provided by the
//! application.
//!
//! Data is stored as a list of sections, each denoted by a single byte ID.
//!
//! When updating the data, one or more sections may be provided to store. Any section IDs found in
//! the current flash page which are not included in the update list will be copied over, so that it
//! is possible to store multiple sections, and to update one without losing the others.

#![allow(dead_code)]

use core::convert::Infallible;

use embedded_io::Read;

/// Magic number to include in the flash page as a header
const MAGIC: u32 = 0xAAAACAFE;

struct Fletcher16 {
    a: u8,
    b: u8,
}

impl Fletcher16 {
    pub fn new() -> Self {
        Self { a: 0, b: 0 }
    }

    pub fn push_byte(&mut self, x: u8) {
        self.a = self.a.overflowing_add(x).0;
        self.b = self.b.overflowing_add(self.a).0;
    }

    pub fn push_slice(&mut self, data: &[u8]) {
        for x in data {
            self.push_byte(*x);
        }
    }

    pub fn value(&self) -> u16 {
        ((self.a as u16) << 8) | self.b as u16
    }

    pub fn compute(data: &[u8]) -> u16 {
        let mut chk = Self::new();
        chk.push_slice(data);
        chk.value()
    }
}

pub enum PersistWriteError {
    OutOfSpace,
}

#[derive(Clone, Copy, Debug, PartialEq, defmt::Format)]
pub enum Page {
    A,
    B,
}

impl Page {
    pub fn other(&self) -> Self {
        match self {
            Page::A => Page::B,
            Page::B => Page::A,
        }
    }
}

/// A trait to be implemented by a flash driver to allow writing persistent data
pub trait FlashAccess {
    type Error;

    /// Get one of the pages as a slice
    fn page(&self, page: Page) -> &[u8];

    fn set_write_page(&mut self, page: Page);

    /// Erase the active write page
    fn erase(&mut self);

    /// Write some data to the active write page
    fn write(&mut self, data: &[u8]);

    /// Must be called after completing all writes
    fn finalize(&mut self);
}

/// Wraps different ways of acquiring data for writing to persist
pub enum UpdateSource<'a> {
    /// Section data is available as a slice
    Slice(&'a [u8]),
    /// Section data is avialble
    Reader((&'a mut dyn Read<Error = Infallible>, usize)),
}

impl<'a> UpdateSource<'a> {
    pub fn len(&self) -> usize {
        match self {
            UpdateSource::Slice(data) => data.len(),
            UpdateSource::Reader((_, size)) => *size,
        }
    }
}

pub struct SectionUpdate<'a> {
    pub section_id: u8,
    pub data: UpdateSource<'a>,
}

/// Attempt to read one of the page as a slice
///
/// The return value will be None if the page does not contain valid data with matching checksum.
///
/// The returned slice will contain just the section data, and does not include the page headers, or checksum.
fn read_page<E>(flash: &dyn FlashAccess<Error = E>, page: Page) -> Option<&'static [u8]> {
    let data = flash.page(page);
    if data.len() < 6 {
        return None;
    }
    let magic = u32::from_le_bytes(data[0..4].try_into().unwrap());
    if magic != MAGIC {
        return None;
    }

    let length = u16::from_le_bytes(data[4..6].try_into().unwrap()) as usize;
    // The total flash section must contain length bytes + 4 byte magic + 2 byte length + 2 byte checksum
    if data.len() < length + PAGE_OVERHEAD {
        return None;
    }

    let chk_offset = PAGE_HEADER_SIZE + length;
    let chk = Fletcher16::compute(&data[..chk_offset]);
    let readback_chk = u16::from_le_bytes(
        data[chk_offset..chk_offset + CHECKSUM_SIZE]
            .try_into()
            .unwrap(),
    );

    if chk == readback_chk {
        // Safety: Converting slice lifetime to 'static is fine, flash will be there
        Some(unsafe { core::mem::transmute(&data[PAGE_HEADER_SIZE..chk_offset]) })
    } else {
        defmt::warn!(
            "Failed persist checksum. Computed: 0x{:x}, read: 0x{:x}",
            chk,
            readback_chk
        );
        None
    }
}

pub struct Section<'a> {
    pub section_id: u8,
    pub data: &'a [u8],
}

pub struct SectionIterator<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> SectionIterator<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
}

impl<'a> Iterator for SectionIterator<'a> {
    type Item = Section<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining() < SECTION_OVERHEAD {
            return None;
        }

        let mut len =
            u16::from_le_bytes(self.data[self.pos..self.pos + 2].try_into().unwrap()) as usize;
        let section_id = self.data[self.pos + 2];
        self.pos += 3;

        // the section_id we just consumed is included in this length, so account for it
        len -= 1;
        if self.remaining() < len {
            defmt::warn!("Persist section came up too short in section iterator");
            return None;
        }
        let new_slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Some(Section {
            section_id,
            data: new_slice,
        })
    }
}

/// Attempt to load persistent data from the flash
///
/// It will look for a valid page, and if found, return it in the form of a [SectionIterator], which
/// allows iterating over each section contained in the page
pub fn load_sections<E>(flash: &dyn FlashAccess<Error = E>) -> Option<SectionIterator> {
    if let Some(page) = read_page(flash, Page::A) {
        defmt::info!("Loading persist from Page A");
        Some(SectionIterator::new(page))
    } else if let Some(page) = read_page(flash, Page::B) {
        defmt::info!("Loading persist from Page B");
        Some(SectionIterator::new(page))
    } else {
        defmt::info!("No valid persist page was found");
        None
    }
}

pub fn write_section(write: &mut dyn FnMut(&[u8]), section: &mut SectionUpdate) -> usize {
    // Each section contains a single type byte, and the content
    let section_size = section.data.len() as u16 + 1;
    write(&section_size.to_le_bytes());
    write(&[section.section_id]);
    match &mut section.data {
        UpdateSource::Slice(slice) => write(slice),
        UpdateSource::Reader(reader) => {
            let mut buf = [0; 32];
            loop {
                let n = reader.0.read(&mut buf).unwrap();
                write(&buf[..n]);
                if n < buf.len() {
                    break;
                }
            }
        }
    }
    // We wrote 2 bytes length header, plus the section data
    section_size as usize + 2
}

// 2 byte length header, 1 byte section id
const SECTION_OVERHEAD: usize = 3;
/// 4-byte magnic number + 2 byte length
const PAGE_HEADER_SIZE: usize = 6;
const CHECKSUM_SIZE: usize = 2;
const PAGE_OVERHEAD: usize = PAGE_HEADER_SIZE + CHECKSUM_SIZE;

pub fn update_sections<E>(
    flash: &mut dyn FlashAccess<Error = E>,
    sections: &mut [SectionUpdate],
) -> Result<(), PersistWriteError> {
    let page_a = read_page(flash, Page::A);
    let page_b = read_page(flash, Page::B);

    let (write_page, read_page) = if page_a.is_some() {
        defmt::info!("Storing persist: Page A valid, writing to Page B");
        (Page::B, page_a)
    } else if page_b.is_some() {
        defmt::info!("Storing persist: Page B valid, writing to Page A");
        (Page::A, page_b)
    } else {
        defmt::info!("Storing persist: No valid pages, writing to Page A");
        (Page::A, None)
    };

    // Figure out how many bytes we will be copying over from existing flash
    // Start with the minimum of magic + length
    let mut copy_bytes = 0;
    if let Some(read_page) = read_page {
        let existing_sections = SectionIterator::new(read_page);
        for section in existing_sections {
            // Skip any sections we are currently updating
            if sections.iter().any(|s| s.section_id == section.section_id) {
                continue;
            }
            // +1 for the section id, +2 for the length header
            copy_bytes += section.data.len() + SECTION_OVERHEAD;
        }
    }

    let mut write_bytes = 0;
    for section in sections.iter() {
        write_bytes += section.data.len() + SECTION_OVERHEAD;
    }

    // Total write len, not including top-level headers and checksum at the end
    let payload_len = copy_bytes + write_bytes;

    if payload_len + PAGE_OVERHEAD > flash.page(write_page).len() {
        return Err(PersistWriteError::OutOfSpace);
    }

    flash.set_write_page(write_page);

    // Copy existing, unchanged sections first
    flash.erase();

    let mut check = Fletcher16::new();
    let mut write = |buf: &[u8]| {
        flash.write(buf);
        check.push_slice(buf);
    };
    write(&MAGIC.to_le_bytes());
    write(&(payload_len as u16).to_le_bytes());
    if let Some(read_page) = read_page {
        let existing_sections = SectionIterator::new(read_page);
        for section in existing_sections {
            // Skip any sections we are currently updating
            if sections.iter().any(|s| s.section_id == section.section_id) {
                continue;
            }
            write_section(
                &mut write,
                &mut SectionUpdate {
                    section_id: section.section_id,
                    data: UpdateSource::Slice(&section.data),
                },
            );
        }
    }

    // Write new sections
    for section in sections {
        write_section(&mut write, section);
    }

    let chksum = check.value();
    flash.write(&chksum.to_le_bytes());
    flash.finalize();

    if read_page.is_some() {
        // Clear the first 8 bytes marking the page as invalid
        flash.set_write_page(write_page.other());
        flash.write(&[0; 8]);
        flash.finalize();
    }

    Ok(())
}
