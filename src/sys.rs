use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::instructions::port::Port;
use crate::println;
use crate::task::executor::EXECUTOR;
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

    term.write_str("=== Core System Report ===\n");
    term.write_str(&format!("Uptime: {:02}:{:02}:{:02}\n", hours, mins, secs));
    term.write_str(&format!("CPU Usage: {}%\n", cpu_usage));
    term.write_str(&format!("CPU Temperature: {}°C\n", cpu_temp));
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