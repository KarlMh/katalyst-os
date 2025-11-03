#![no_std]
#![no_main]

extern crate alloc;

use blog_os::println;
use blog_os::task::{Task, executor::Executor};
use blog_os::repl::katalyst_repl;
use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    use blog_os::allocator;
    use blog_os::memory::{self, BootInfoFrameAllocator};
    use x86_64::VirtAddr;

    blog_os::init();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };
    allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");

    // Auto-load snapshot at boot (I/O path hardened)
    if blog_os::fs::persist::load_from_disk().is_ok() {
        blog_os::println!("[fs] loaded snapshot");
    } else {
        blog_os::println!("[fs] no snapshot");
    }

    let mut executor = Executor::new();
    executor.spawn(Task::new(katalyst_repl()));
    executor.run();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    blog_os::hlt_loop();
}
