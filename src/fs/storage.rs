use super::dir::Directory;
use alloc::vec::Vec;
use spin::Mutex;
use lazy_static::lazy_static;
use alloc::boxed::Box;

/// Root directory
lazy_static! {
    pub static ref ROOT_DIR: Mutex<Directory> = Mutex::new({
        let mut root = Directory::new("home"); // home is root

        for &name in ["docs", "downloads", "media", "vault", "logs"].iter() {
            root.subdirs.insert(name, Box::new(Directory::new(name)));
        }

        root
    });
}




/// Simple "disk" simulation
pub struct Disk {
    pub blocks: Vec<Vec<u8>>, // each Vec<u8> is a block
}

impl Disk {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    pub fn write_block(&mut self, data: &[u8]) {
        self.blocks.push(data.to_vec());
    }

    pub fn read_block(&self, index: usize) -> Option<&[u8]> {
        self.blocks.get(index).map(|v| v.as_slice())
    }
}
