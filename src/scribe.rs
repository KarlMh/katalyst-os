use crate::terminal::Terminal;
use crate::task::keyboard::ScancodeStream;
use pc_keyboard::{layouts, DecodedKey, KeyCode, Keyboard, ScancodeSet1};
use crate::fs::storage::ROOT_DIR;
use crate::fs::commands::write_file;

use alloc::{string::String, vec::Vec};
use core::ops::{Deref, DerefMut};
use crate::alloc::string::ToString;
use crate::repl::{resolve_cwd, resolve_cwd_mut};
use futures_util::StreamExt;



const WIDTH: usize = 80;
const HEIGHT: usize = 25;

/// Core Scribe editor: a lightweight in-terminal line editor.
pub struct Scribe<'a> {
    pub term: &'a mut Terminal,
    pub filename: &'a str,
    pub lines: Vec<String>,
    pub cur_line: usize,
    pub cur_col_char: usize,
    pub top_line: usize,
}

impl<'a> Deref for Scribe<'a> {
    type Target = Terminal;
    fn deref(&self) -> &Self::Target { self.term }
}

impl<'a> DerefMut for Scribe<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.term }
}

impl<'a> Scribe<'a> {
    pub fn new(term: &'a mut Terminal, filename: &'a str, cwd_path: &mut Vec<&'static str>) -> Self {
        let mut lines = Vec::new();

        if filename.is_empty() {
            term.write_str("Invalid filename\n");
        } else {
            let root = ROOT_DIR.lock();
            let cwd = resolve_cwd(&root, cwd_path);
            if let Some(f) = cwd.files.get(filename) {
                if let Ok(s) = core::str::from_utf8(&f.content) {
                    for l in s.split('\n') {
                        lines.push(l.to_string());
                    }
                    if s.ends_with('\n') {
                        lines.push(String::new());
                    }
                } else {
                    term.write_str("<binary file, cannot edit>\n");
                }
            }
        }

        if lines.is_empty() {
            lines.push(String::new());
        }

        Self {
            term,
            filename,
            lines,
            cur_line: 0,
            cur_col_char: 0,
            top_line: 0,
        }
    }

    fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
        s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
    }

    fn byte_idx_to_char_idx(s: &str, byte_idx: usize) -> usize {
        s[..byte_idx.min(s.len())].chars().count()
    }

    fn redraw(&mut self) {
        self.clear_screen();
        let total = self.lines.len();
        for i in 0..HEIGHT {
            let idx = self.top_line + i;
            if idx >= total { break; }
            let line = self.lines[idx].clone();
            for c in line.chars().take(WIDTH) {
                self.write_char(c);
            }

            self.write_char('\n');
        }
    }

    fn save_and_quit(&mut self, cwd_path: &mut Vec<&'static str>) {
        let mut root = ROOT_DIR.lock();
        let cwd = resolve_cwd_mut(&mut root, cwd_path);
        let joined = {
            let mut s = String::new();
            for (i, l) in self.lines.iter().enumerate() {
                if i == self.cur_line && l.trim() == "&q" { break; }
                s.push_str(l);
                if i + 1 < self.lines.len() { s.push('\n'); }
            }
            s
        };
        write_file(self.term, cwd, self.filename, joined.as_bytes());
        self.write_str("\nSaved & exiting Scribe...\n");
    }

    async fn handle_input(
        &mut self,
        scancodes: &mut ScancodeStream,
        keyboard: &mut Keyboard<layouts::Us104Key, ScancodeSet1>,
        cwd_path: &mut Vec<&'static str>,
    ) -> bool {
        if let Some(scancode) = scancodes.next().await {
            if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                if let Some(key) = keyboard.process_keyevent(key_event) {
                    match key {
                        DecodedKey::Unicode(c) => match c {
                            '\n' | '\r' => {
                                if self.lines[self.cur_line].trim() == "&q" {
                                    self.save_and_quit(cwd_path);
                                    return true;
                                }
                                let byte_idx = Self::char_to_byte_idx(&self.lines[self.cur_line], self.cur_col_char);
                                let cur = self.lines[self.cur_line].clone();
                                let (left, right) = cur.split_at(byte_idx);
                                self.lines[self.cur_line] = left.to_string();
                                self.lines.insert(self.cur_line + 1, right.to_string());
                                self.cur_line += 1;
                                self.cur_col_char = 0;
                                self.redraw();
                            }
                            '\x08' => {
                                if self.cur_col_char > 0 {
                                    let byte_idx = Self::char_to_byte_idx(&self.lines[self.cur_line], self.cur_col_char);
                                    let prev_byte = Self::char_to_byte_idx(&self.lines[self.cur_line], self.cur_col_char - 1);
                                    self.lines[self.cur_line].drain(prev_byte..byte_idx);
                                    self.cur_col_char -= 1;
                                } else if self.cur_line > 0 {
                                    let prev_len = self.lines[self.cur_line - 1].chars().count();
                                    let tail = self.lines.remove(self.cur_line);
                                    self.cur_line -= 1;
                                    self.cur_col_char = prev_len;
                                    self.lines[self.cur_line].push_str(&tail);
                                }
                                self.redraw();
                            }
                            _ => {
                                let byte_idx = Self::char_to_byte_idx(&self.lines[self.cur_line], self.cur_col_char);
                                let mut s = self.lines[self.cur_line].clone();
                                s.insert_str(byte_idx, &c.to_string());
                                self.lines[self.cur_line] = s;
                                self.cur_col_char += 1;
                                self.redraw();
                            }
                        },
                        DecodedKey::RawKey(code) => match code {
                            KeyCode::ArrowUp => {
                                if self.cur_line > 0 {
                                    self.cur_line -= 1;
                                    let len = self.lines[self.cur_line].chars().count();
                                    if self.cur_col_char > len { self.cur_col_char = len; }
                                }
                            }
                            KeyCode::ArrowDown => {
                                if self.cur_line + 1 < self.lines.len() {
                                    self.cur_line += 1;
                                    let len = self.lines[self.cur_line].chars().count();
                                    if self.cur_col_char > len { self.cur_col_char = len; }
                                }
                            }
                            KeyCode::ArrowLeft => {
                                if self.cur_col_char > 0 {
                                    self.cur_col_char -= 1;
                                } else if self.cur_line > 0 {
                                    self.cur_line -= 1;
                                    self.cur_col_char = self.lines[self.cur_line].chars().count();
                                }
                            }
                            KeyCode::ArrowRight => {
                                let len = self.lines[self.cur_line].chars().count();
                                if self.cur_col_char < len {
                                    self.cur_col_char += 1;
                                } else if self.cur_line + 1 < self.lines.len() {
                                    self.cur_line += 1;
                                    self.cur_col_char = 0;
                                }
                            }
                            KeyCode::Delete => {
                                let len = self.lines[self.cur_line].chars().count();
                                if self.cur_col_char < len {
                                    let bstart = Self::char_to_byte_idx(&self.lines[self.cur_line], self.cur_col_char);
                                    let bend = Self::char_to_byte_idx(&self.lines[self.cur_line], self.cur_col_char + 1);
                                    self.lines[self.cur_line].drain(bstart..bend);
                                } else if self.cur_line + 1 < self.lines.len() {
                                    let next = self.lines.remove(self.cur_line + 1);
                                    self.lines[self.cur_line].push_str(&next);
                                }
                                self.redraw();
                            }
                            KeyCode::Home => { self.cur_col_char = 0; }
                            KeyCode::End => { self.cur_col_char = self.lines[self.cur_line].chars().count(); }
                            _ => {}
                        },
                    }
                }
            }
        }
        false
    }

    pub async fn run(
        &mut self,
        scancodes: &mut ScancodeStream,
        keyboard: &mut Keyboard<layouts::Us104Key, ScancodeSet1>,
        cwd_path: &mut Vec<&'static str>,
    ) {
        self.redraw();

        loop {
            // keep cursor visible
            if self.cur_line < self.top_line {
                self.top_line = self.cur_line;
                self.redraw();
            } else if self.cur_line >= self.top_line + HEIGHT {
                self.top_line = self.cur_line.saturating_sub(HEIGHT - 1);
                self.redraw();
            }

            let rel_line = self.cur_line.saturating_sub(self.top_line);
            let byte_idx = Self::char_to_byte_idx(&self.lines[self.cur_line], self.cur_col_char);
            let col = Self::byte_idx_to_char_idx(&self.lines[self.cur_line], byte_idx);
            self.cursor_x = col;
            self.cursor_y = rel_line;
            self.move_cursor();

            if self.handle_input(scancodes, keyboard, cwd_path).await {
                break;
            }
        }

        self.clear_input();
        self.clear_screen();
        self.redraw_input();
    }
}
