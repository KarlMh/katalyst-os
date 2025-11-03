extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::fmt::Write;

use crate::task::keyboard::ScancodeStream;
use pc_keyboard::{DecodedKey, Keyboard, ScancodeSet1, layouts, HandleControl};
use futures_util::stream::StreamExt;

use crate::fs::commands::{spawn_file_folder, despawn_file_folder, scan_files};
use crate::fs::storage::ROOT_DIR;
use crate::fs::dir::Directory;
use crate::fs::file::File;

use crate::alloc::string::ToString;

use alloc::format;
use alloc::vec;

const VGA_BUFFER: *mut u8 = 0xb8000 as *mut u8;
const WIDTH: usize = 80;
const HEIGHT: usize = 25;

pub struct Terminal {
    cursor_x: usize,
    cursor_y: usize,
    input: String,
    prompt: String,
}

impl Terminal {
    fn new(prompt: &str) -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            input: String::new(),
            prompt: prompt.to_string(),
        }
    }

    fn clear_screen(&mut self) {
        unsafe {
            for i in 0..(WIDTH*HEIGHT*2) {
                VGA_BUFFER.add(i).write_volatile(0);
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    fn move_cursor(&self) {
        let pos = (self.cursor_y * WIDTH + self.cursor_x) as u16;
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3d4u16, in("al") 0x0fu8);
            core::arch::asm!("out dx, al", in("dx") 0x3d5u16, in("al") (pos & 0xff) as u8);
            core::arch::asm!("out dx, al", in("dx") 0x3d4u16, in("al") 0x0eu8);
            core::arch::asm!("out dx, al", in("dx") 0x3d5u16, in("al") (pos >> 8) as u8);
        }
    }

    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => {
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= HEIGHT {
                    self.scroll_up();
                    self.cursor_y = HEIGHT - 1;
                }
            }
            _ => {
                let offset = 2 * (self.cursor_y * WIDTH + self.cursor_x);
                unsafe {
                    VGA_BUFFER.add(offset).write_volatile(c as u8);
                    VGA_BUFFER.add(offset + 1).write_volatile(0x0f);
                }
                self.cursor_x += 1;
                if self.cursor_x >= WIDTH {
                    self.cursor_x = 0;
                    self.cursor_y += 1;
                    if self.cursor_y >= HEIGHT {
                        self.scroll_up();
                        self.cursor_y = HEIGHT - 1;
                    }
                }
            }
        }
        self.move_cursor();
    }

    pub fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    fn redraw_input(&mut self) {
        let line = format!("{}{}", self.prompt, self.input);
        for i in 0..WIDTH {
            let offset = 2 * (self.cursor_y * WIDTH + i);
            unsafe {
                if i < line.len() {
                    VGA_BUFFER.add(offset).write_volatile(line.as_bytes()[i]);
                    VGA_BUFFER.add(offset+1).write_volatile(0x0f);
                } else {
                    VGA_BUFFER.add(offset).write_volatile(b' ');
                    VGA_BUFFER.add(offset+1).write_volatile(0x0f);
                }
            }
        }
        self.cursor_x = line.len();
        self.move_cursor();
    }

    fn push(&mut self, c: char) {
        self.input.push(c);
        self.redraw_input();
    }

    fn pop(&mut self) {
        if self.input.pop().is_some() {
            self.redraw_input();
        }
    }

    fn clear_input(&mut self) {
        self.input.clear();
        self.redraw_input();
    }

    fn get_input(&self) -> &str {
        &self.input
    }

    fn scroll_up(&mut self) {
        unsafe {
            for y in 1..HEIGHT {
                for x in 0..WIDTH {
                    let from = 2 * (y * WIDTH + x);
                    let to = 2 * ((y-1) * WIDTH + x);
                    VGA_BUFFER.add(to).write_volatile(VGA_BUFFER.add(from).read_volatile());
                    VGA_BUFFER.add(to+1).write_volatile(VGA_BUFFER.add(from+1).read_volatile());
                }
            }
            for x in 0..WIDTH {
                let offset = 2 * ((HEIGHT-1) * WIDTH + x);
                VGA_BUFFER.add(offset).write_volatile(b' ');
                VGA_BUFFER.add(offset+1).write_volatile(0x0f);
            }
        }
    }
}

/// Main REPL
pub async fn katalyst_repl() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);

    let mut term = Terminal::new("");
    term.clear_screen();

    term.write_str("katalyst v0.1\n");
    term.write_str("a simple OS kernel, made by kewl.\n\n");

    let mut cwd_path: Vec<&'static str> = vec!["main"];

    loop {
        term.clear_input();
        update_prompt(&mut term, &cwd_path);
        term.move_cursor();

        // Read input
        loop {
            if let Some(scancode) = scancodes.next().await {
                if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                    if let Some(key) = keyboard.process_keyevent(key_event) {
                        match key {
                            DecodedKey::Unicode(c) => match c {
                                '\n' | '\r' => { term.cursor_x = 0; term.cursor_y += 1; term.move_cursor(); break; }
                                '\x08' => term.pop(),
                                _ => term.push(c),
                            },
                            DecodedKey::RawKey(_) => {}
                        }
                    }
                }
            }
        }

        let input = term.get_input().trim().to_string();
        let mut parts = input.split_whitespace();
        let command = parts.next().unwrap_or("");
        let arg = parts.next();

        match command {
            "help" => {
                // All lines are &'static str
                let help_text: [&'static str; 3] = [
                    "System commands: core, halt, reboot, spark",
                    "File commands: make file/folder, del file/folder, peek folder, move source -> dest",
                    "Other file commands: push filename content, pull filename, link source dest, clone source dest",
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
            "spark" => crate::sys::spark(&mut term),
            "core" => crate::sys::core_report(&mut term),
            "here" => {
                term.write_str(&format!("Current directory: {}\n", cwd_path.join("/")));
            }

            "make" => {
                if let Some(folder) = arg {
                    let mut root = ROOT_DIR.lock();
                    let cwd = resolve_cwd_mut(&mut root, &cwd_path);
                    spawn_file_folder(&mut term, cwd, folder);
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
                scan_files(&mut term, &root_ref, &cwd_path, arg);
            }





            "->" => {
                if let Some(target) = arg {
                    let root = ROOT_DIR.lock();
                    let mut temp = &*root;
                    let mut path_stack = vec![temp.name];

                    if target.starts_with('/') {
                        // Absolute path
                        let parts: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
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
                        if success { cwd_path = path_stack; }
                    } else {
                        // Relative path
                        for part in cwd_path.iter().skip(1) { temp = temp.subdirs.get(part).unwrap(); }

                        if let Some(child) = temp.subdirs.get(target) {
                            cwd_path.push(child.name);
                        } else {
                            term.write_str(&format!("Directory '{}' not found\n", target));
                        }
                    }
                } else {
                    term.write_str("Usage: -> <dir>\n");
                }
            },

            "<-" => {
                if cwd_path.len() > 1 {
                    cwd_path.pop();
                } else {
                    term.write_str("Already at root\n");
                }
            },


            _ => term.write_str("Unknown command\n"),
        }
    }
}

fn resolve_cwd<'a>(root: &'a Directory, cwd_path: &[&'static str]) -> &'a Directory {
    let mut temp = root;
    for part in cwd_path.iter().skip(1) {
        temp = temp.subdirs.get(part).unwrap();
    }
    temp
}

fn resolve_cwd_mut<'a>(root: &'a mut Directory, cwd_path: &[&'static str]) -> &'a mut Directory {
    let mut temp = root;
    for part in cwd_path.iter().skip(1) {
        temp = temp.subdirs.get_mut(part).unwrap();
    }
    temp
}


fn update_prompt(term: &mut Terminal, cwd_path: &[&str]) {
    term.prompt = format!("katalyst@{}=> ", cwd_path.join("/"));
    term.redraw_input(); // redraws prompt + current input
}
