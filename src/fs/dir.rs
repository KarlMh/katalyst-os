use super::file::File;
use alloc::collections::BTreeMap;
use alloc::boxed::Box;
use alloc::{vec::Vec, string::String};

use crate::alloc::string::ToString;

pub struct Directory {
    pub name: &'static str,
    pub files: BTreeMap<&'static str, File>,
    pub subdirs: BTreeMap<&'static str, Box<Directory>>, // new: subdirectories
}

impl Directory {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            files: BTreeMap::new(),
            subdirs: BTreeMap::new(),
        }
    }

    // Remove file by name
    pub fn remove_file(&mut self, name: &str) -> Option<File> {
        self.files.remove(name)
    }

    // Remove subdirectory by name
    pub fn remove_subdir(&mut self, name: &str) -> Option<Directory> {
        self.subdirs.remove(name).map(|boxed_dir| *boxed_dir)
    }

    // Add a file
    pub fn add_file(&mut self, file: File) {
        let key = Box::leak(file.name.clone().into_boxed_str());
        self.files.insert(key, file);
    }

    // Add a subdirectory
    pub fn add_subdir(&mut self, dir: Directory) {
        let key = Box::leak(dir.name.to_string().into_boxed_str());
        self.subdirs.insert(key, Box::new(dir));
    }

    // Get a file by name
    pub fn get_file(&self, name: &str) -> Option<&File> {
        self.files.get(name)
    }

    // Get a mutable reference to a subdirectory
    pub fn get_subdir_mut(&mut self, name: &str) -> Option<&mut Directory> {
        self.subdirs.get_mut(name).map(|b| b.as_mut())
    }

    // List all files
    pub fn list_files(&self) -> Vec<String> {
        self.files.iter().map(|f| f.1.name.clone()).collect()
    }

    // List all subdirectories
    pub fn list_subdirs(&self) -> Vec<String> {
        self.subdirs.iter().map(|(_, d)| d.name.to_string()).collect()
    }
}
