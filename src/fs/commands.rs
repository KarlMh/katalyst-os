use super::storage::{ROOT_DIR};
use super::file::File;
use super::dir::Directory;
use crate::repl::Terminal;
use crate::alloc::string::ToString;

use alloc::{boxed::Box, format, string::String, vec, vec::Vec};

/// Create a new file or folder and persist changes
pub fn spawn_file_folder(term: &mut Terminal, parent_dir: &mut Directory, name: &str) {
    if name.is_empty() {
        term.write_str("Name cannot be empty!\n");
        return;
    }

    if name.contains('.') {
        // Treat as file
        let file = File::new(name);
        parent_dir.add_file(file);
        term.write_str(&format!("Created file '{}'\n", name));
    } else {
        // Treat as folder
        let static_name: &'static str = Box::leak(name.to_string().into_boxed_str());

        let dir = Directory::new(static_name);
        parent_dir.add_subdir(dir);
        term.write_str(&format!("Created folder '{}'\n", name));
    }
}

/// Delete a file or folder and persist changes
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

/// Scan files and directories
pub fn scan_files(term: &mut Terminal, root: &Directory, cwd_path: &[&str], path: Option<&str>) {
    // Determine target directory
    let mut temp: &Directory;
    let mut path_stack = vec![root.name.clone()];

    if let Some(p) = path {
        if p.is_empty() || p == "root" {
            temp = root;
        } else {
            // Split and traverse
            let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
            temp = root;
            let mut temp_ref = root;
            for part in parts.iter() {
                if let Some(child) = temp_ref.subdirs.get(*part) {
                    temp_ref = child;
                    path_stack.push(child.name.clone());
                } else {
                    term.write_str(&format!("Directory '{}' not found\n", part));
                    return;
                }
            }
            temp = temp_ref;
        }
    } else {
        // Use current working directory
        let mut temp_ref = root;
        for part in cwd_path.iter().skip(1) {
            if let Some(child) = temp_ref.subdirs.get(*part) {
                temp_ref = child;
                path_stack.push(child.name.clone());
            }
        }
        temp = temp_ref;
    }

    // Show path
    term.write_str(&format!("/{} -> ", path_stack.join("/")));

    // Print folders
    for d in temp.list_subdirs() {
        term.write_str(&format!("/{0} ", d));
    }

    // Print files
    for f in temp.list_files() {
        term.write_str(&format!("{} ", f));
    }

    if temp.subdirs.is_empty() && temp.files.is_empty() {
        term.write_str("(empty)");
    }

    term.write_char('\n');
}

/// Create a new file or folder based on name
pub fn make_file(term: &mut Terminal, parent_dir: &mut Directory, name: &str) {
    if name.is_empty() { term.write_str("Name cannot be empty!\n"); return; }
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

/// Print file contents (utf8 best-effort) or list a directory; if name is None, list cwd
pub fn peek_path(term: &mut Terminal, cwd: &Directory, name: Option<&str>) {
    match name {
        Some(n) => {
            if let Some(f) = cwd.files.get(n) {
                match core::str::from_utf8(&f.content) {
                    Ok(s) => { term.write_str(s); term.write_char('\n'); }
                    Err(_) => term.write_str("<binary>\n"),
                }
            } else if let Some(d) = cwd.subdirs.get(n) {
                // Show files and subdirs of the subdir, mark dirs with '/'
                for sub in d.list_subdirs().iter() { term.write_str(&format!("{}/\n", sub)); }
                for file in d.list_files().iter() { term.write_str(file); term.write_char('\n'); }
            } else { term.write_str("Not found\n"); }
        }
        None => {
            // List current dir
            for sub in cwd.list_subdirs().iter() { term.write_str(&format!("{}/\n", sub)); }
            for file in cwd.list_files().iter() { term.write_str(file); term.write_char('\n'); }
        }
    }
}

/// Clear file content
pub fn void_file(term: &mut Terminal, parent_dir: &mut Directory, name: &str) {
    if let Some(file) = parent_dir.files.get_mut(name) {
        file.content.clear();
        term.write_str("Cleared\n");
    } else { term.write_str("File not found\n"); }
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
        term.write_str("Saved.\n");
    }
}

/// Simple subslice search (naive)
pub fn find_subslice(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() { return true; }
    if needle.len() > hay.len() { return false; }
    for i in 0..=hay.len()-needle.len() {
        if &hay[i..i+needle.len()] == needle { return true; }
    }
    false
}

/// Search all files in cwd for pattern; print filenames that match
pub fn seek_in_cwd(term: &mut Terminal, cwd: &Directory, pattern: &[u8]) {
    for (name, f) in cwd.files.iter() {
        if find_subslice(&f.content, pattern) {
            term.write_str(&format!("{}\n", name));
        }
    }
}
