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
use crate::scribe::Scribe;

use crate::alloc::string::ToString;

use alloc::format;
use alloc::vec;

const VGA_BUFFER: *mut u8 = 0xb8000 as *mut u8;
const WIDTH: usize = 80;
const HEIGHT: usize = 25;

pub struct Terminal {
    pub(crate) cursor_x: usize,  // VGA col
    pub(crate) cursor_y: usize,  // VGA row
    input: String,    // complete text input content
    input_cursor: usize, // input cursor pos (bytes, NOT screen pos)
    pub(crate) prompt: String,
    history: Vec<String>,
    hist_pos: Option<usize>,
}

impl Terminal {
    pub fn new(prompt: &str) -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            input: String::new(),
            input_cursor: 0,
            prompt: prompt.to_string(),
            history: Vec::new(),
            hist_pos: None,
        }
    }

    pub(crate) fn clear_screen(&mut self) {
        unsafe {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let offset = 2 * (y * WIDTH + x);
                    VGA_BUFFER.add(offset).write_volatile(b' ');
                    VGA_BUFFER.add(offset + 1).write_volatile(0x0f);
                }
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    pub(crate) fn move_cursor(&self) {
        let pos = (self.cursor_y * WIDTH + self.cursor_x) as u16;
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3d4u16, in("al") 0x0fu8);
            core::arch::asm!("out dx, al", in("dx") 0x3d5u16, in("al") (pos & 0xff) as u8);
            core::arch::asm!("out dx, al", in("dx") 0x3d4u16, in("al") 0x0eu8);
            core::arch::asm!("out dx, al", in("dx") 0x3d5u16, in("al") (pos >> 8) as u8);
        }
    }

    pub(crate) fn write_char(&mut self, c: char) {
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

    pub(crate) fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    /// redraw the prompt+input, with cursor placed at current input_cursor
    pub(crate) fn redraw_input(&mut self) {
        let line = format!("{}{}", self.prompt, self.input);
        let prompt_len = self.prompt.chars().count();
        // Clear this whole line
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
        // New: set VGA cursor to column = prompt.len() + cursor logical pos in input
        // To correctly place for unicode: count chars, but input_cursor is counted in bytes,
        // so we must count chars up to input_cursor bytes.
        let input_cursor_char_idx = self.input[..self.input_cursor].chars().count();
        self.cursor_x = prompt_len + input_cursor_char_idx;
        self.move_cursor();
    }

    /// Insert a char at input_cursor
    pub(crate) fn push(&mut self, c: char) {
        // Prevent line break in the terminal input line
        if c == '\n' || c == '\r' {
            return;
        }
        // Insert at cursor position
        let mut s = self.input.clone();
        let input_byte_pos = self.input_cursor;
        s.insert(input_byte_pos, c);
        self.input = s;
        self.input_cursor += c.len_utf8();
        self.redraw_input();
    }

    /// Delete the char before input_cursor (backspace)
    pub(crate) fn pop(&mut self) {
        if self.input_cursor == 0 {
            // nothing to do
            return;
        }
        // Find previous char boundary (unicode-safe)
        let prev = self.input[..self.input_cursor].char_indices().rev().next();
        if let Some((idx, _ch)) = prev {
            let mut s = self.input.clone();
            s.drain(idx..self.input_cursor);
            self.input = s;
            self.input_cursor = idx;
            self.redraw_input();
        }
    }

    /// Delete the char at cursor position (Delete key)
    pub(crate) fn del_forward(&mut self) {
        // Find next char boundary
        let next = self.input[self.input_cursor..].char_indices().nth(0);
        if let Some((rel_idx, ch)) = next {
            let byte_start = self.input_cursor + rel_idx;
            let byte_end = byte_start + ch.len_utf8();
            let mut s = self.input.clone();
            s.drain(byte_start..byte_end);
            self.input = s;
            self.redraw_input();
        }
    }

    /// Move cursor left by one char (if possible)
    pub(crate) fn move_input_cursor_left(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let prev = self.input[..self.input_cursor].char_indices().rev().next();
        if let Some((idx, _ch)) = prev {
            self.input_cursor = idx;
            self.redraw_input();
        }
    }

    /// Move cursor right by one char (if possible)
    pub(crate) fn move_input_cursor_right(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }
        let next = self.input[self.input_cursor..].char_indices().next();
        if let Some((idx, ch)) = next {
            self.input_cursor += idx + ch.len_utf8();
            self.redraw_input();
        }
    }

    /// Move cursor to beginning (Home key)
    pub(crate) fn move_input_cursor_home(&mut self) {
        self.input_cursor = 0;
        self.redraw_input();
    }

    /// Move cursor to end (End key)
    pub(crate) fn move_input_cursor_end(&mut self) {
        self.input_cursor = self.input.len();
        self.redraw_input();
    }

    pub(crate) fn clear_input(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
        self.redraw_input();
    }

    pub(crate) fn get_input(&self) -> &str {
        &self.input
    }

    pub(crate) fn set_input(&mut self, s: &str) {
        self.input.clear();
        self.input.push_str(s);
        self.input_cursor = self.input.len();
        self.redraw_input();
    }

    pub(crate) fn history_push(&mut self, s: &str) {
        if !s.is_empty() {
            self.history.push(s.to_string());
        }
        self.hist_pos = None;
    }

    pub(crate) fn history_prev(&mut self) {
        if self.history.is_empty() { return; }
        let idx = match self.hist_pos { Some(i) => i.saturating_sub(1), None => self.history.len().saturating_sub(1) };
        let line = self.history[idx].clone();
        self.hist_pos = Some(idx);
        self.set_input(&line);
    }

    pub(crate) fn history_next(&mut self) {
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

    pub(crate) fn scroll_up(&mut self) {
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

