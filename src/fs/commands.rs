use super::storage::ROOT_DIR;
use super::file::File;
use super::dir::Directory;
use crate::terminal::Terminal;
use crate::alloc::string::ToString;
use alloc::boxed::Box;
use alloc::format;

use alloc::{string::String, vec::Vec};

/// Create a new file or folder
pub fn make_file(term: &mut Terminal, parent_dir: &mut Directory, name: &str) {
    if name.is_empty() {
        term.write_str("Name cannot be empty!\n");
        return;
    }

    if name.contains('.') {
        let file = File::new(name);
        parent_dir.add_file(file);
        term.write_str(&format!("Created file '{}'\n", name));
    } else {
        let static_name: &'static str = Box::leak(name.to_string().into_boxed_str());
        let dir = Directory::new(static_name);
        parent_dir.add_subdir(dir);
        term.write_str(&format!("Created folder '{}'\n", name));
    }
}

/// Delete a file or folder
pub fn despawn_file_folder(term: &mut Terminal, parent_dir: &mut Directory, name: &str) {
    if name.is_empty() {
        term.write_str("Name cannot be empty!\n");
        return;
    }

    let removed = if name.contains('.') {
        parent_dir.remove_file(name).is_some()
    } else {
        parent_dir.remove_subdir(name).is_some()
    };

    if removed {
        term.write_str(&format!("Deleted '{}'\n", name));
    } else {
        term.write_str(&format!("'{}' not found\n", name));
    }
}

/// Print file contents or list a directory (in-line)
pub fn peek_path(term: &mut Terminal, cwd: &Directory, name: Option<&str>) {
    match name {
        Some(n) => {
            if let Some(f) = cwd.files.get(n) {
                match core::str::from_utf8(&f.content) {
                    Ok(s) => term.write_str(s),
                    Err(_) => term.write_str("<binary>"),
                }
            } else if let Some(d) = cwd.subdirs.get(n) {
                let mut items = Vec::new();
                for sub in d.list_subdirs() { items.push(format!("{}/", sub)); }
                for file in d.list_files() { items.push(file); }
                if items.is_empty() { items.push("(empty)".to_string()); }
                term.write_str(&items.join(" "));
            } else {
                term.write_str("Not found");
            }
        }
        None => {
            let mut items = Vec::new();
            for sub in cwd.list_subdirs() { items.push(format!("{}/", sub)); }
            for file in cwd.list_files() { items.push(file); }
            if items.is_empty() { items.push("(empty)".to_string()); }
            term.write_str(&items.join(" "));
        }
    }
    term.write_char('\n');
}


/// Clear file content
pub fn void_file(term: &mut Terminal, parent_dir: &mut Directory, name: &str) {
    if let Some(file) = parent_dir.files.get_mut(name) {
        file.content.clear();
        term.write_str("Cleared\n");
    } else {
        term.write_str("File not found\n");
    }
}

/// Overwrite file with bytes; creates if missing
pub fn write_file(term: &mut Terminal, parent_dir: &mut Directory, name: &str, bytes: &[u8]) {
    if !parent_dir.files.contains_key(name) {
        let f = File::new(name);
        let key: &'static str = Box::leak(name.to_string().into_boxed_str());
        parent_dir.files.insert(key, f);
    }
    if let Some(file) = parent_dir.files.get_mut(name) {
        file.content.clear();
        file.content.extend_from_slice(bytes);
    }
}

/// Simple subslice search
pub fn find_subslice(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() { return true; }
    if needle.len() > hay.len() { return false; }
    for i in 0..=hay.len() - needle.len() {
        if &hay[i..i + needle.len()] == needle { return true; }
    }
    false
}

/// Search all files in cwd for pattern
pub fn seek_in_cwd(term: &mut Terminal, cwd: &Directory, pattern: &[u8]) {
    for (name, f) in cwd.files.iter() {
        if find_subslice(&f.content, pattern) {
            term.write_str(&format!("{} ", name));
        }
    }
    term.write_char('\n');
}
