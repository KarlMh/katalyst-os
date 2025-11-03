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
