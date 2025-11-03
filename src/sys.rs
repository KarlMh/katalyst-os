use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::instructions::port::Port;
use crate::println;
use crate::fs::persist::save_to_disk;
use crate::task::executor::EXECUTOR;
use crate::fs::storage::ROOT_DIR;
use crate::fs::persist::{LAST_SNAPSHOT_TICKS, LAST_SNAPSHOT_BYTES};
use crate::block::ata::ata_present;
use crate::fs::dir::Directory;
use alloc::format;

/// Total ticks since boot
pub static UPTIME_TICKS: AtomicU64 = AtomicU64::new(0);

/// Ticks spent idling (hlt)
pub static IDLE_TICKS: AtomicU64 = AtomicU64::new(0);

/// Timer frequency: how many times the timer fires per second
pub const TICKS_PER_SECOND: u64 = 1000;

use crate::repl::Terminal;


pub fn spark(term: &mut Terminal) {
    term.write_str("System spark initiated.\n");
}

pub fn halt(term: &mut Terminal) -> ! {
    match save_to_disk() {
        Ok(()) => term.write_str("Auto-saved.\n"),
        Err(()) => term.write_str("Auto-save failed.\n"),
    }
    term.write_str("System halted.\n");

    loop {
        x86_64::instructions::hlt();
    }
    
}

/// Get uptime in hours, minutes, and seconds
pub fn get_uptime() -> (u64, u64, u64) {
    let ticks = UPTIME_TICKS.load(Ordering::Relaxed);
    let total_seconds = ticks / TICKS_PER_SECOND;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    (hours, minutes, seconds)
}

/// Calculate CPU usage in percent (0-100)
pub fn get_cpu_usage() -> u8 {
    let total = UPTIME_TICKS.load(Ordering::Relaxed);
    let idle = IDLE_TICKS.load(Ordering::Relaxed);

    if total == 0 {
        0
    } else {
        (((total - idle) as u128 * 100) / total as u128) as u8
    }
}

/// Base temperature when idle (in Celsius)
const BASE_TEMP: u8 = 35;
/// Maximum temperature under full load
const MAX_TEMP: u8 = 85;

/// Get a simulated CPU temperature based on idle vs uptime
pub fn get_cpu_temperature() -> u8 {
    let total = UPTIME_TICKS.load(Ordering::Relaxed);
    let idle = IDLE_TICKS.load(Ordering::Relaxed);

    if total == 0 {
        return BASE_TEMP;
    }

    let usage = (((total - idle) as u128 * 100) / total as u128) as u8;
    BASE_TEMP + ((MAX_TEMP - BASE_TEMP) as u16 * usage as u16 / 100) as u8
}

/// Reboot the system via the keyboard controller
pub fn reboot(term: &mut Terminal) -> ! {
    match save_to_disk() {
        Ok(()) => term.write_str("Auto-saved.\n"),
        Err(()) => term.write_str("Auto-save failed.\n"),
    }
    term.write_str("System rebooting...\n");
    unsafe {
        let mut port = Port::new(0x64);
        port.write(0xFEu8); // pulse reset line
    }

    loop {
        x86_64::instructions::hlt();
    }

    

}

/// Prints a system core diagnostics report
pub fn core_report(term: &mut Terminal) {
    let cpu_usage = get_cpu_usage();
    let cpu_temp = get_cpu_temperature();
    let (hours, mins, secs) = get_uptime();
    let snapshot_age_secs = {
        let last = LAST_SNAPSHOT_TICKS.load(Ordering::Relaxed);
        let now = UPTIME_TICKS.load(Ordering::Relaxed);
        now.saturating_sub(last) / TICKS_PER_SECOND
    };
    let snapshot_bytes = LAST_SNAPSHOT_BYTES.load(Ordering::Relaxed);

    // FS stats
    fn walk(dir: &Directory) -> (u64, u64, u64) {
        let mut dirs = 1u64; // count self
        let mut files = 0u64;
        let mut bytes = 0u64;
        for (_n, f) in dir.files.iter() {
            files += 1;
            bytes += f.content.len() as u64;
        }
        for (_n, d) in dir.subdirs.iter() {
            let (cd, cf, cb) = walk(d);
            dirs += cd;
            files += cf;
            bytes += cb;
        }
        (dirs, files, bytes)
    }
    let (dirs, files, bytes) = {
        let root = ROOT_DIR.lock();
        walk(&root)
    };

    term.write_str("=== Core System Report ===\n");
    term.write_str(&format!("Uptime: {:02}:{:02}:{:02}\n", hours, mins, secs));
    term.write_str(&format!("CPU Usage: {}%\n", cpu_usage));
    term.write_str(&format!("CPU Temperature: {}°C\n", cpu_temp));
    term.write_str(&format!("Disk: {}\n", if ata_present() { "attached" } else { "not detected" }));
    term.write_str(&format!("Snapshot: {} bytes, age: {}s\n", snapshot_bytes, snapshot_age_secs));
    term.write_str(&format!("FS: {} dirs, {} files, {} bytes\n", dirs, files, bytes));
    term.write_str("Active Tasks:\n");

    let exec = EXECUTOR.lock();
    let task_count = exec.task_count();

    if task_count > 0 {
        for id in exec.task_ids() {
            term.write_str(&format!("- Task ID: {}\n", id));
        }
    } else {
        term.write_str("No active tasks.\n");
    }

    term.write_str("=========================\n");
}