use alloc::string::String;
use alloc::vec::Vec;

pub struct File {
    pub name: String,
    pub content: Vec<u8>, // stored in memory; flush to disk for persistence
}

impl File {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            content: Vec::new(),
        }
    }

    pub fn write(&mut self, data: &[u8]) {
        self.content.extend_from_slice(data);
    }

    pub fn read(&self) -> &[u8] {
        &self.content
    }
}
