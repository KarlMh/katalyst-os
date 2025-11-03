extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::fmt::Write;

use crate::task::keyboard::ScancodeStream;
use pc_keyboard::{DecodedKey, Keyboard, ScancodeSet1, layouts, HandleControl, KeyCode};
use futures_util::stream::StreamExt;

use crate::fs::commands::{despawn_file_folder, make_file, peek_path, void_file, write_file, seek_in_cwd};
use crate::fs::persist::{save_to_disk, load_from_disk};
use crate::sys::{UPTIME_TICKS, TICKS_PER_SECOND};
use crate::fs::storage::ROOT_DIR;
use crate::fs::dir::Directory;
// use crate::fs::file::File;

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
    history: Vec<String>,
    hist_pos: Option<usize>,
}

impl Terminal {
    fn new(prompt: &str) -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            input: String::new(),
            prompt: prompt.to_string(),
            history: Vec::new(),
            hist_pos: None,
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

    fn set_input(&mut self, s: &str) {
        self.input.clear();
        self.input.push_str(s);
        self.redraw_input();
    }

    fn history_push(&mut self, s: &str) {
        if !s.is_empty() {
            self.history.push(s.to_string());
        }
        self.hist_pos = None;
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() { return; }
        let idx = match self.hist_pos { Some(i) => i.saturating_sub(1), None => self.history.len().saturating_sub(1) };
        let line = self.history[idx].clone();
        self.hist_pos = Some(idx);
        self.set_input(&line);
    }

    fn history_next(&mut self) {
        if self.history.is_empty() { return; }
        if let Some(i) = self.hist_pos {
            if i + 1 < self.history.len() {
                let line = self.history[i + 1].clone();
                self.hist_pos = Some(i + 1);
                self.set_input(&line);
            } else {
                self.hist_pos = None;
                self.set_input("");
            }
        }
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
                                '\n' | '\r' => { term.cursor_x = 0; term.cursor_y += 1; term.move_cursor(); break; }
                                '\t' => autocomplete(&mut term, &cwd_path),
                                '\x08' => term.pop(),
                                _ => term.push(c),
                            },
                            DecodedKey::RawKey(code) => match code {
                                KeyCode::ArrowUp => term.history_prev(),
                                KeyCode::ArrowDown => term.history_next(),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        let input = term.get_input().trim().to_string();
        term.history_push(&input);
        let mut parts = input.split_whitespace();
        let command = parts.next().unwrap_or("");
        let arg = parts.next();

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
            "spark" => crate::sys::spark(&mut term),
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
                    term.write_str("Enter text. End with a single line '::end'\n");
                    let content = read_multiline(&mut term, &mut scancodes, &mut keyboard).await;
                    let mut root = ROOT_DIR.lock();
                    let cwd = resolve_cwd_mut(&mut root, &cwd_path);
                    write_file(&mut term, cwd, name, content.as_bytes());
                } else { term.write_str("Usage: scribe <file>\n"); }
            }

            "seek" => {
                if let Some(pattern) = arg {
                    let root_ref = ROOT_DIR.lock();
                    let cwd = resolve_cwd(&root_ref, &cwd_path);
                    seek_in_cwd(&mut term, cwd, pattern.as_bytes());
                } else { term.write_str("Usage: seek <pattern>\n"); }
            }





            "->" => {
                if let Some(target) = arg {
                    let root = ROOT_DIR.lock();
                    let mut temp = &*root;
                    let mut path_stack = vec![temp.name];
                    let trimmed = target.trim_start_matches([' ', '/'].as_ref());
                    if trimmed.is_empty() {
                        return;
                    }
                    let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
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

// Simple autocomplete: complete last token from command/file/dir names
fn autocomplete(term: &mut Terminal, cwd_path: &[&'static str]) {
    let input = term.get_input().to_string();
    let parts: Vec<&str> = input.split_whitespace().collect();
    let (prefix, token) = if let Some(last) = parts.last() {
        let start = input.rfind(last).unwrap_or(0);
        (input[..start].to_string(), (*last).to_string())
    } else {
        (String::new(), String::new())
    };

    let mut candidates: Vec<String> = Vec::new();
    // commands
    let cmds = ["help","halt","reboot","spark","core","save","load","here","make","del","peek","void","scribe","seek","->","<-"];
    for c in cmds.iter() { if c.starts_with(&token) { candidates.push((*c).to_string()); } }
    // files/dirs in cwd
    let root = ROOT_DIR.lock();
    let cwd = resolve_cwd(&root, cwd_path);
    for (name, _) in cwd.files.iter() { if name.starts_with(&token) { candidates.push((*name).to_string()); } }
    for (name, _) in cwd.subdirs.iter() { if name.starts_with(&token) { candidates.push((*name).to_string()); } }

    if candidates.len() == 1 {
        let completed = &candidates[0];
        let sep = if prefix.is_empty() { "" } else { " " };
        term.set_input(&format!("{}{}{}", prefix.trim_end(), sep, completed));
    }
}

// Read lines until '::end' line
async fn read_multiline(term: &mut Terminal, scancodes: &mut ScancodeStream, keyboard: &mut Keyboard<layouts::Us104Key, ScancodeSet1>) -> String {
    let mut out = String::new();
    loop {
        let mut line = String::new();
        loop {
            if let Some(sc) = scancodes.next().await {
                if let Ok(Some(ev)) = keyboard.add_byte(sc) {
                    if let Some(key) = keyboard.process_keyevent(ev) {
                        match key {
                            DecodedKey::Unicode(c) => match c {
                                '\n' | '\r' => { term.write_char('\n'); break; }
                                '\x08' => { let _ = line.pop(); /* no redraw for simplicity */ },
                                _ => { line.push(c); term.write_char(c); }
                            },
                            _ => {}
                        }
                    }
                }
            }
        }
        if line.trim() == "::end" { break; }
        out.push_str(&line);
        out.push('\n');
    }
    out
}
