extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::fmt::Write;
use crate::println;
use crate::task::keyboard::ScancodeStream;
use pc_keyboard::{DecodedKey, Keyboard, ScancodeSet1, layouts, HandleControl, KeyCode};
use futures_util::stream::StreamExt;

use crate::fs::commands::{despawn_file_folder, make_file, peek_path, void_file, write_file, seek_in_cwd};
use crate::fs::persist::{save_to_disk, load_from_disk};
use crate::sys::{UPTIME_TICKS, TICKS_PER_SECOND};
use crate::fs::storage::ROOT_DIR;
use crate::fs::dir::Directory;
use crate::scribe::Scribe;


use crate::alloc::string::ToString;

use alloc::format;
use alloc::vec;

use crate::terminal::Terminal;
use crate::vga_buffer::{Color, ColorCode, ScreenChar};
use alloc::boxed::Box;



/// Main REPL
pub async fn katalyst_repl() {

    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);

    let mut term = Terminal::new("");
    term.clear_screen();

    term.write_str(r"

               __              __           .__                     __   
              |  | _______   _/  |_ _____   |  |   ___.__.  _______/  |_ 
              |  |/ /\__  \  \   __\\__  \  |  |  <   |  | /  ___/\   __\
              |    <  / __ \_ |  |   / __ \_|  |__ \___  | \___ \  |  |  
              |__|_ \(____  / |__|  (____  /|____/ / ____|/____  > |__|  
                   \/     \/             \/        \/          \/      
                          K   A   T   A   L   Y   S   T 
                              Built for Pure Focus.          
                                      v0.1

   ");
   

    let mut cwd_path: Vec<&'static str> = vec!["home"];
    let mut last_autosave_ticks: u64 = 0;

    loop {
        term.clear_input();
        update_prompt(&mut term, &cwd_path);
        term.move_cursor();

        // Periodic autosave (~every 10 seconds)
        let now = UPTIME_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if now.saturating_sub(last_autosave_ticks) >= 10 * TICKS_PER_SECOND {
            match save_to_disk() {
                Ok(()) => term.write_str("[auto] saved\n"),
                Err(()) => term.write_str("[auto] save failed\n"),
            }
            last_autosave_ticks = now;
        }

        // Read input
        loop {
            if let Some(scancode) = scancodes.next().await {
                if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                    if let Some(key) = keyboard.process_keyevent(key_event) {
                        match key {
                            DecodedKey::Unicode(c) => match c {
                                '\n' | '\r' => { 
                                    term.cursor_x = 0; 
                                    term.cursor_y += 1; 
                                    term.move_cursor(); 
                                    break; 
                                }
                                '\t' => autocomplete(&mut term, &cwd_path),
                                '\x08' => term.pop(),
                                // Control chars are ignored, eg. delete handled as RawKey
                                _ => term.push(c),
                            },
                            DecodedKey::RawKey(code) => match code {
                                KeyCode::F2 => {
                                    // insert "->" at cursor position
                                    let mut s = term.input.clone();
                                    s.insert_str(term.input_cursor, "->");
                                    term.input = s;
                                    term.input_cursor += 2;
                                    term.redraw_input();
                                }
                                KeyCode::F3 => {
                                    // insert "<-" at cursor position
                                    let mut s = term.input.clone();
                                    s.insert_str(term.input_cursor, "<-");
                                    term.input = s;
                                    term.input_cursor += 2;
                                    term.redraw_input();
                                }
                                // existing keys
                                KeyCode::ArrowLeft => term.move_input_cursor_left(),
                                KeyCode::ArrowRight => term.move_input_cursor_right(),
                                KeyCode::ArrowUp => term.history_prev(),
                                KeyCode::ArrowDown => term.history_next(),
                                KeyCode::Delete => term.del_forward(),
                                KeyCode::Home => term.move_input_cursor_home(),
                                KeyCode::End => term.move_input_cursor_end(),
                                _ => {}
                            }
                            
                        }
                    }
                }
            }
        }

        let input = term.get_input().trim().to_string();

        // Skip empty input (nothing typed or just spaces)
        if input.is_empty() {
            continue; // go back to the top of the REPL loop
        }
        
        term.history_push(&input);
        let mut parts = input.split_whitespace();
        let command = parts.next().unwrap_or("");
        let arg = parts.next();


        if input.starts_with("->") {
            let rest_of_line = input.trim_start_matches("->").trim();
            if rest_of_line.is_empty() {
                term.write_str("Usage: -> <dir>\n");
                continue;
            }
        
            let root = ROOT_DIR.lock();
            // Start from cwd, not always root
            let mut temp = resolve_cwd(&root, &cwd_path);
            let mut path_stack = cwd_path.clone(); // keep current cwd
        
            // split by '/' for nested paths
            let parts: Vec<&str> = rest_of_line.split('/').filter(|s| !s.is_empty()).collect();
            let mut success = true;
        
            for part in parts.iter() {
                if let Some(child) = temp.subdirs.get(part) {
                    temp = child;
                    path_stack.push(child.name);
                } else {
                    term.write_str(&format!("Directory '{}' not found\n", part));
                    success = false;
                    break;
                }
            }
        
            if success {
                cwd_path = path_stack;
            }
        
            continue;
        }
        
        
        

        match command {
            "help" => {
                // All lines are &'static str
                let help_text: [&'static str; 4] = [
                    "System: core, halt, reboot, spark, save, load",
                    "Navigation: here, -> <dir>, <-",
                    "Files: make <name>, del <name>, peek [file|dir], void <file>",
                    "Edit/search: scribe <file>, seek <pattern>",
                ];

                // Print each line followed by a newline
                for line in help_text.iter() {
                    term.write_str(line);
                    term.write_char('\n');
                }
            }

            "wipe" | "wp" => term.clear_screen(),
            "halt" => crate::sys::halt(&mut term),
            "reboot" => crate::sys::reboot(&mut term),
            "spark" => {
                term.clear_screen();
                term.write_str(r"

               __              __           .__                     __   
              |  | _______   _/  |_ _____   |  |   ___.__.  _______/  |_ 
              |  |/ /\__  \  \   __\\__  \  |  |  <   |  | /  ___/\   __\
              |    <  / __ \_ |  |   / __ \_|  |__ \___  | \___ \  |  |  
              |__|_ \(____  / |__|  (____  /|____/ / ____|/____  > |__|  
                   \/     \/             \/        \/          \/      
                          K   A   T   A   L   Y   S   T 
                              Built for Pure Focus.          
                                      v0.1
 
    ");

            }

            "core" => crate::sys::core_report(&mut term),
            "save" => {
                term.write_str("Saving...\n");
                match save_to_disk() {
                    Ok(()) => term.write_str("Saved to disk\n"),
                    Err(()) => term.write_str("Save failed\n"),
                }
            }
            "load" => {
                match load_from_disk() {
                    Ok(()) => term.write_str("Loaded from disk\n"),
                    Err(()) => term.write_str("Load failed\n"),
                }
            }
            "here" => {
                term.write_str(&format!("Current directory: {}\n", cwd_path.join("/")));
            }

            "make" => {
                if let Some(folder) = arg {
                    let mut root = ROOT_DIR.lock();
                    let cwd = resolve_cwd_mut(&mut root, &cwd_path);
                    make_file(&mut term, cwd, folder);
                } else { term.write_str("Invalid spawn syntax. Use: spawn foldername\n"); }
            }

            "del" => {
                if let Some(folder) = arg {
                    let mut root = ROOT_DIR.lock();
                    let cwd = resolve_cwd_mut(&mut root, &cwd_path);
                    despawn_file_folder(&mut term, cwd, folder);
                } else { term.write_str("Invalid despawn syntax. Use: despawn foldername\n"); }
            }

            "peek" => {
                let root_ref = ROOT_DIR.lock();
                let cwd = resolve_cwd(&root_ref, &cwd_path);
                peek_path(&mut term, cwd, arg);
            }

            "void" => {
                if let Some(name) = arg {
                    let mut root = ROOT_DIR.lock();
                    let cwd = resolve_cwd_mut(&mut root, &cwd_path);
                    void_file(&mut term, cwd, name);
                } else { term.write_str("Usage: void <file>\n"); }
            }

            "scribe" => {
                if let Some(name) = arg {
                    // enter scribe editor mode for given filename
                    let mut scribe = Scribe::new(&mut term, name, &mut cwd_path);
                    scribe.run(&mut scancodes, &mut keyboard, &mut cwd_path).await;

                    // Save to disk after exiting scribe
                    term.clear_screen();
                    match save_to_disk() {
                        Ok(()) => term.write_str("[scribe] saved.\n"),
                        Err(()) => term.write_str("[scribe] save failed.\n"),
                    }
                } else {
                    term.write_str("Usage: scribe <filename>\n");
                }
            }

            "parse" => {
                if let Some(name) = arg {
                    let root = ROOT_DIR.lock();
                    let cwd = resolve_cwd(&root, &cwd_path);

                    if let Some(file) = cwd.files.get(name) {
                        let nodes = crate::fs::ink::parse_ink(file);
                        crate::fs::ink::render_ink_vga(&mut term, &nodes);
                    } else {
                        term.write_str("File not found.\n");
                    }
                } else {
                    term.write_str("Usage: parse <file>\n");
                }
            }











            "seek" => {
                if let Some(pattern) = arg {
                    let root_ref = ROOT_DIR.lock();
                    let cwd = resolve_cwd(&root_ref, &cwd_path);
                    seek_in_cwd(&mut term, cwd, pattern.as_bytes());
                } else { term.write_str("Usage: seek <pattern>\n"); }
            }


            "reverse" | "rev" => {
                // Accept full line after command as argument, properly reversing multi-word phrases
                let rest_of_line = input.trim_start_matches(command).trim();
                if !rest_of_line.is_empty() {
                    let reversed: String = rest_of_line.chars().rev().collect();
                    term.write_str(&reversed);
                    term.write_str("\n");
                } else {
                    term.write_str("Usage: reverse <text>\n");
                }
            }






            "<-" => {
                let rest_of_line = input.trim_start_matches(command).trim();
                if !rest_of_line.is_empty() {
                    term.write_str("Usage: <- takes no arguments\n");
                } else if cwd_path.len() > 1 {
                    cwd_path.pop();
                } else {
                    term.write_str("Already at root\n");
                }
            }


            
            _ => term.write_str("Unknown command\n"),
        }
    }
}

pub fn resolve_cwd<'a>(root: &'a Directory, cwd_path: &[&'static str]) -> &'a Directory {
    let mut temp = root;
    for part in cwd_path.iter().skip(1) {
        temp = temp.subdirs.get(part).unwrap();
    }
    temp
}

pub fn resolve_cwd_mut<'a>(root: &'a mut Directory, cwd_path: &[&'static str]) -> &'a mut Directory {
    let mut temp = root;
    for part in cwd_path.iter().skip(1) {
        temp = temp.subdirs.get_mut(part).unwrap();
    }
    temp
}





pub fn update_prompt(term: &mut Terminal, cwd_path: &[&str]) {
    // Store plain-text version of prompt for input handling
    term.prompt = format!("katalyst@/{}=> ", cwd_path.join("/"));

    // Immediately redraw input after updating prompt
    term.redraw_input();
}

fn autocomplete(term: &mut Terminal, cwd_path: &[&'static str]) {
    let input = term.get_input().to_string();

    // Determine start of last token (space or start of line)
    let token_start = input.rfind(|c: char| c == ' ' || c == '\t').map(|i| i + 1).unwrap_or(0);
    let token_slice = &input[token_start..];

    // Detect special prefixes for navigation
    let (token_prefix, token) = if token_slice.starts_with("->") {
        ("->", &token_slice[2..])
    } else if token_slice.starts_with("<-") {
        ("<-", &token_slice[2..])
    } else {
        ("", token_slice)
    };

    // Keep everything before the token (used for replacement)
    let prefix = &input[..token_start];

    let mut candidates: Vec<String> = Vec::new();

    // 1. All commands
    let cmds = [
        "help","halt","reboot","spark","core","save","load","here",
        "make","del","peek","void","scribe","seek","reverse",
        "->","<-","wipe","parse"
    ];
    for c in cmds.iter() {
        if c.starts_with(token) {
            candidates.push((*c).to_string());
        }
    }

    // 2. Files & directories in the current working directory
    let root = ROOT_DIR.lock();
    let cwd = resolve_cwd(&root, cwd_path);
    for (name, _) in cwd.files.iter() {
        if name.starts_with(token) {
            candidates.push((*name).to_string());
        }
    }
    for (name, _) in cwd.subdirs.iter() {
        if name.starts_with(token) {
            candidates.push((*name).to_string());
        }
    }

    // 3. Nested path autocompletion for -><dir>/<subdir>
    if token_prefix == "->" && token.contains('/') {
        let mut path_parts: Vec<&str> = token.split('/').collect();
        let last_part = path_parts.pop().unwrap_or("");
        let mut temp = resolve_cwd(&root, cwd_path);
        let mut valid_path = true;

        // Traverse all but the last part
        for part in &path_parts {
            if let Some(sub) = temp.subdirs.get(*part) {
                temp = sub;
            } else {
                valid_path = false;
                break;
            }
        }

        if valid_path {
            for (name, _) in temp.subdirs.iter() {
                if name.starts_with(last_part) {
                    let completed_path = if path_parts.is_empty() {
                        format!("{}{}", token_prefix, name)
                    } else {
                        format!("{}{}{}", token_prefix, path_parts.join("/"), format!("/{}", name))
                    };
                    candidates.push(completed_path);
                }
            }
        }
    }

    // Only autocomplete if exactly one candidate exists
    if candidates.len() == 1 {
        let replacement = format!("{}{}{} ", prefix, token_prefix, candidates[0].trim_start_matches(token_prefix));
        term.set_input(&replacement);
    }
}
