#![allow(dead_code)]

use core::arch::asm;
use x86_64::instructions::interrupts;
use spin::Mutex;
use lazy_static::lazy_static;

const ATA_PRIMARY_IO: u16 = 0x1F0;
const ATA_PRIMARY_CTRL: u16 = 0x3F6;

const REG_DATA: u16 = ATA_PRIMARY_IO + 0;
const REG_ERROR_FEATURES: u16 = ATA_PRIMARY_IO + 1;
const REG_SECTOR_COUNT: u16 = ATA_PRIMARY_IO + 2;
const REG_LBA0: u16 = ATA_PRIMARY_IO + 3;
const REG_LBA1: u16 = ATA_PRIMARY_IO + 4;
const REG_LBA2: u16 = ATA_PRIMARY_IO + 5;
const REG_DRIVE_HEAD: u16 = ATA_PRIMARY_IO + 6;
const REG_STATUS_COMMAND: u16 = ATA_PRIMARY_IO + 7;

const REG_ALT_STATUS_DEVCTRL: u16 = ATA_PRIMARY_CTRL + 0;

lazy_static! {
    static ref ATA_LOCK: Mutex<()> = Mutex::new(());
}

const STATUS_ERR: u8 = 1 << 0;
const STATUS_DRQ: u8 = 1 << 3;
const STATUS_SRV: u8 = 1 << 4;
const STATUS_DF: u8 = 1 << 5;
const STATUS_RDY: u8 = 1 << 6;
const STATUS_BSY: u8 = 1 << 7;

const CMD_READ_SECTORS: u8 = 0x20;
const CMD_WRITE_SECTORS: u8 = 0x30;

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags)); }
    value
}

#[inline]
unsafe fn outb(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe { asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags)); }
    value
}

#[inline]
unsafe fn outw(port: u16, value: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags)); }
}

fn ata_wait_ready() -> Result<(), ()> {
    // 400ns delay by reading alt status 4 times
    unsafe {
        let _ = inb(REG_ALT_STATUS_DEVCTRL);
        let _ = inb(REG_ALT_STATUS_DEVCTRL);
        let _ = inb(REG_ALT_STATUS_DEVCTRL);
        let _ = inb(REG_ALT_STATUS_DEVCTRL);
    }
    for _ in 0..1000000 {
        let status = unsafe { inb(REG_STATUS_COMMAND) };
        if status == 0x00 || status == 0xFF { continue; }
        if status & STATUS_BSY != 0 { continue; }
        if status & STATUS_ERR != 0 || status & STATUS_DF != 0 { return Err(()); }
        if status & STATUS_RDY != 0 { return Ok(()); }
    }
    Err(())
}

pub fn ata_present() -> bool {
    // Many emulators/devices return 0xFF on nonexistent ports
    let mut same = 0u8;
    let mut last = 0u8;
    for _ in 0..8 {
        let v = unsafe { inb(REG_STATUS_COMMAND) };
        if v == last { same = same.saturating_add(1); } else { same = 0; }
        last = v;
    }
    if last == 0xFF { return false; }
    true
}

fn ata_wait_drq() -> Result<(), ()> {
    for _ in 0..1000000 {
        let status = unsafe { inb(REG_STATUS_COMMAND) };
        if status & STATUS_ERR != 0 || status & STATUS_DF != 0 { return Err(()); }
        if status & STATUS_DRQ != 0 { return Ok(()); }
        if status & STATUS_BSY != 0 { continue; }
    }
    Err(())
}

pub fn read_lba28(lba: u32, sector_count: u8, buffer: &mut [u8]) -> Result<(), ()> {
    if sector_count == 0 { return Ok(()); }
    if buffer.len() < (sector_count as usize) * 512 { return Err(()); }

    let _g = ATA_LOCK.lock();
    interrupts::without_interrupts(|| {
        // select drive and check presence, then wait ready
        unsafe {
            outb(REG_DRIVE_HEAD, 0xF0 | (((lba >> 24) & 0x0F) as u8));
            // disable drive interrupts (nIEN)
            outb(REG_ALT_STATUS_DEVCTRL, 0x02);
        }
        if !ata_present() { return Err(()); }
        ata_wait_ready()?;
        unsafe {
            outb(REG_SECTOR_COUNT, sector_count);
            outb(REG_LBA0, (lba & 0xFF) as u8);
            outb(REG_LBA1, ((lba >> 8) & 0xFF) as u8);
            outb(REG_LBA2, ((lba >> 16) & 0xFF) as u8);
            outb(REG_STATUS_COMMAND, CMD_READ_SECTORS);
        }

        for s in 0..sector_count {
            ata_wait_drq()?;
            for i in 0..256u16 {
                let word = unsafe { inw(REG_DATA) };
                let offset = (s as usize) * 512 + (i as usize) * 2;
                buffer[offset] = (word & 0xFF) as u8;
                buffer[offset + 1] = (word >> 8) as u8;
            }
        }

        Ok(())
    })
}

pub fn write_lba28(lba: u32, sector_count: u8, data: &[u8]) -> Result<(), ()> {
    if sector_count == 0 { return Ok(()); }
    if data.len() < (sector_count as usize) * 512 { return Err(()); }

    let _g = ATA_LOCK.lock();
    interrupts::without_interrupts(|| {
        unsafe {
            outb(REG_DRIVE_HEAD, 0xF0 | (((lba >> 24) & 0x0F) as u8));
            outb(REG_ALT_STATUS_DEVCTRL, 0x02);
        }
        if !ata_present() { return Err(()); }
        ata_wait_ready()?;
        unsafe {
            outb(REG_SECTOR_COUNT, sector_count);
            outb(REG_LBA0, (lba & 0xFF) as u8);
            outb(REG_LBA1, ((lba >> 8) & 0xFF) as u8);
            outb(REG_LBA2, ((lba >> 16) & 0xFF) as u8);
            outb(REG_STATUS_COMMAND, CMD_WRITE_SECTORS);
        }

        for s in 0..sector_count {
            ata_wait_drq()?;
            for i in 0..256u16 {
                let offset = (s as usize) * 512 + (i as usize) * 2;
                let word = (data[offset] as u16) | ((data[offset + 1] as u16) << 8);
                unsafe { outw(REG_DATA, word); }
            }
        }

        ata_wait_ready()?;
        Ok(())
    })
}


