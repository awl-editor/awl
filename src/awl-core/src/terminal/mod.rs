pub mod view;

use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::Sender;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use vte::{Params, Parser, Perform};

use crate::app::events::AppEvent;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum TermColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Default)]
pub struct TermCell {
    pub ch: char,
    pub fg: TermColor,
    pub bg: TermColor,
    pub bold: bool,
}

pub struct TermState {
    pub cols: usize,
    pub rows: usize,
    pub screen: Vec<Vec<TermCell>>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scrollback: Vec<Vec<TermCell>>,
    pub scroll_offset: usize,
    pub pending_title: Option<String>,
    pen_fg: TermColor,
    pen_bg: TermColor,
    pen_bold: bool,
}

impl TermState {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            screen: vec![vec![TermCell::default(); cols.max(1)]; rows.max(1)],
            cursor_row: 0,
            cursor_col: 0,
            scrollback: Vec::new(),
            scroll_offset: 0,
            pending_title: None,
            pen_fg: TermColor::Default,
            pen_bg: TermColor::Default,
            pen_bold: false,
        }
    }

    #[allow(dead_code)]
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        self.cols = cols;
        self.rows = rows;
        self.screen.resize(rows, vec![TermCell::default(); cols]);
        for row in &mut self.screen {
            row.resize(cols, TermCell::default());
        }
        self.cursor_row = self.cursor_row.min(rows - 1);
        self.cursor_col = self.cursor_col.min(cols - 1);
    }

    fn newline(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            self.scroll_up();
        } else {
            self.cursor_row += 1;
        }
    }

    fn scroll_up(&mut self) {
        let row = self.screen.remove(0);
        self.scrollback.push(row);
        if self.scrollback.len() > 5000 {
            self.scrollback.remove(0);
        }
        self.screen.push(vec![TermCell::default(); self.cols]);
    }

    fn apply_sgr(&mut self, ps: &[u16]) {
        if ps.is_empty() {
            self.pen_fg = TermColor::Default;
            self.pen_bg = TermColor::Default;
            self.pen_bold = false;
            return;
        }
        let mut i = 0;
        while i < ps.len() {
            match ps[i] {
                0 => {
                    self.pen_fg = TermColor::Default;
                    self.pen_bg = TermColor::Default;
                    self.pen_bold = false;
                }
                1 => self.pen_bold = true,
                2 | 22 => self.pen_bold = false,
                30..=37 => self.pen_fg = TermColor::Indexed(ps[i] as u8 - 30),
                38 => {
                    if ps.get(i + 1).copied() == Some(5) {
                        if let Some(&n) = ps.get(i + 2) {
                            self.pen_fg = TermColor::Indexed(n as u8);
                            i += 2;
                        }
                    } else if ps.get(i + 1).copied() == Some(2) {
                        if let (Some(&r), Some(&g), Some(&b)) = (ps.get(i + 2), ps.get(i + 3), ps.get(i + 4)) {
                            self.pen_fg = TermColor::Rgb(r as u8, g as u8, b as u8);
                            i += 4;
                        }
                    }
                }
                39 => self.pen_fg = TermColor::Default,
                40..=47 => self.pen_bg = TermColor::Indexed(ps[i] as u8 - 40),
                48 => {
                    if ps.get(i + 1).copied() == Some(5) {
                        if let Some(&n) = ps.get(i + 2) {
                            self.pen_bg = TermColor::Indexed(n as u8);
                            i += 2;
                        }
                    } else if ps.get(i + 1).copied() == Some(2) {
                        if let (Some(&r), Some(&g), Some(&b)) = (ps.get(i + 2), ps.get(i + 3), ps.get(i + 4)) {
                            self.pen_bg = TermColor::Rgb(r as u8, g as u8, b as u8);
                            i += 4;
                        }
                    }
                }
                49 => self.pen_bg = TermColor::Default,
                90..=97 => self.pen_fg = TermColor::Indexed(ps[i] as u8 - 90 + 8),
                100..=107 => self.pen_bg = TermColor::Indexed(ps[i] as u8 - 100 + 8),
                _ => {}
            }
            i += 1;
        }
    }
}

impl Perform for TermState {
    fn print(&mut self, c: char) {
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.newline();
        }
        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            self.screen[self.cursor_row][self.cursor_col] = TermCell { ch: c, fg: self.pen_fg, bg: self.pen_bg, bold: self.pen_bold };
        }
        self.cursor_col += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x0A | 0x0B | 0x0C => self.newline(),
            0x0D => self.cursor_col = 0,
            0x08 => self.cursor_col = self.cursor_col.saturating_sub(1),
            0x09 => {
                let next = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next.min(self.cols.saturating_sub(1));
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        let ps: Vec<u16> = params.iter().map(|p| p[0]).collect();
        match action {
            'H' | 'f' => {
                let r = ps.get(0).copied().unwrap_or(1).saturating_sub(1) as usize;
                let c = ps.get(1).copied().unwrap_or(1).saturating_sub(1) as usize;
                self.cursor_row = r.min(self.rows.saturating_sub(1));
                self.cursor_col = c.min(self.cols.saturating_sub(1));
            }
            'A' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            'B' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = (self.cursor_row + n).min(self.rows.saturating_sub(1));
            }
            'C' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                self.cursor_col = (self.cursor_col + n).min(self.cols.saturating_sub(1));
            }
            'D' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            'E' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = (self.cursor_row + n).min(self.rows.saturating_sub(1));
                self.cursor_col = 0;
            }
            'F' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.cursor_col = 0;
            }
            'G' => {
                let c = ps.get(0).copied().unwrap_or(1).saturating_sub(1) as usize;
                self.cursor_col = c.min(self.cols.saturating_sub(1));
            }
            'd' => {
                let r = ps.get(0).copied().unwrap_or(1).saturating_sub(1) as usize;
                self.cursor_row = r.min(self.rows.saturating_sub(1));
            }
            'J' => match ps.get(0).copied().unwrap_or(0) {
                0 => {
                    for c in self.cursor_col..self.cols {
                        self.screen[self.cursor_row][c] = TermCell::default();
                    }
                    for r in (self.cursor_row + 1)..self.rows {
                        self.screen[r] = vec![TermCell::default(); self.cols];
                    }
                }
                1 => {
                    for c in 0..=self.cursor_col.min(self.cols.saturating_sub(1)) {
                        self.screen[self.cursor_row][c] = TermCell::default();
                    }
                    for r in 0..self.cursor_row {
                        self.screen[r] = vec![TermCell::default(); self.cols];
                    }
                }
                2 | 3 => {
                    for r in &mut self.screen {
                        *r = vec![TermCell::default(); self.cols];
                    }
                    self.cursor_row = 0;
                    self.cursor_col = 0;
                }
                _ => {}
            },
            'K' => match ps.get(0).copied().unwrap_or(0) {
                0 => {
                    for c in self.cursor_col..self.cols {
                        self.screen[self.cursor_row][c] = TermCell::default();
                    }
                }
                1 => {
                    for c in 0..=self.cursor_col.min(self.cols.saturating_sub(1)) {
                        self.screen[self.cursor_row][c] = TermCell::default();
                    }
                }
                2 => self.screen[self.cursor_row] = vec![TermCell::default(); self.cols],
                _ => {}
            },
            'L' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                for _ in 0..n {
                    self.screen.insert(self.cursor_row, vec![TermCell::default(); self.cols]);
                    if self.screen.len() > self.rows {
                        self.screen.pop();
                    }
                }
            }
            'M' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                for _ in 0..n.min(self.rows.saturating_sub(self.cursor_row)) {
                    if self.cursor_row < self.screen.len() {
                        self.screen.remove(self.cursor_row);
                        self.screen.push(vec![TermCell::default(); self.cols]);
                    }
                }
            }
            'P' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                if self.cursor_row < self.rows {
                    let row = &mut self.screen[self.cursor_row];
                    let end = (self.cursor_col + n).min(self.cols);
                    if end <= row.len() {
                        row.drain(self.cursor_col..end);
                        row.resize(self.cols, TermCell::default());
                    }
                }
            }
            'S' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                for _ in 0..n {
                    self.scroll_up();
                }
            }
            'T' => {
                let n = ps.get(0).copied().unwrap_or(1).max(1) as usize;
                for _ in 0..n {
                    self.screen.insert(0, vec![TermCell::default(); self.cols]);
                    if self.screen.len() > self.rows {
                        self.screen.pop();
                    }
                }
            }
            'm' => self.apply_sgr(&ps),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        if byte == b'M' {
            // reverse index
            if self.cursor_row == 0 {
                self.screen.insert(0, vec![TermCell::default(); self.cols]);
                if self.screen.len() > self.rows {
                    self.screen.pop();
                }
            } else {
                self.cursor_row -= 1;
            }
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.len() >= 2 && (params[0] == b"0" || params[0] == b"2") {
            if let Ok(title) = std::str::from_utf8(params[1]) {
                let title = title.trim().to_string();
                if !title.is_empty() {
                    self.pending_title = Some(title);
                }
            }
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}

pub struct TerminalPane {
    pub id: usize,
    pub name: String,
    pub state: TermState,
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty>,
    parser: Parser,
}

impl TerminalPane {
    pub fn spawn(cols: usize, rows: usize, cwd: &Path, tx: Sender<AppEvent>, id: usize) -> io::Result<Self> {
        let pty_system = NativePtySystem::default();
        let size = PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 };
        let pair = pty_system.openpty(size).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        pair.slave.spawn_command(cmd).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        let writer = pair.master.take_writer().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        let master = pair.master;

        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(AppEvent::TerminalOutput { id, data: buf[..n].to_vec() }).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let name = format!("terminal {}", id + 1);
        Ok(Self { id, name, state: TermState::new(cols, rows), writer, master, parser: Parser::new() })
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let size = PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 };
        let _ = self.master.resize(size);
        self.state.resize(cols, rows);
    }

    pub fn process(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.parser.advance(&mut self.state, b);
        }
        if let Some(title) = self.state.pending_title.take() {
            self.name = truncate_tab_name(&title);
        }
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }
}

fn truncate_tab_name(s: &str) -> String {
    const MAX: usize = 20;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= MAX {
        s.to_string()
    } else {
        chars[..MAX - 1].iter().collect::<String>() + "…"
    }
}

pub fn indexed_to_rgb(idx: u8) -> (u8, u8, u8) {
    match idx {
        0 => (30, 30, 30),
        1 => (204, 0, 0),
        2 => (78, 154, 6),
        3 => (196, 160, 0),
        4 => (52, 101, 164),
        5 => (117, 80, 123),
        6 => (6, 152, 154),
        7 => (211, 215, 207),
        8 => (85, 87, 83),
        9 => (239, 41, 41),
        10 => (138, 226, 52),
        11 => (252, 233, 79),
        12 => (114, 159, 207),
        13 => (173, 127, 168),
        14 => (52, 226, 226),
        15 => (238, 238, 236),
        16..=231 => {
            let i = idx - 16;
            let b = i % 6;
            let g = (i / 6) % 6;
            let r = i / 36;
            let to_byte = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (to_byte(r), to_byte(g), to_byte(b))
        }
        232..=255 => {
            let level = 8 + (idx - 232) * 10;
            (level, level, level)
        }
    }
}
