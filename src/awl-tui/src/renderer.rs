use std::io::{self, Write};
use std::path::Path;

use crate::buffer::Buffer;
use crate::cell::{Color, UnderlineStyle};

pub struct Renderer {
    current: Buffer,
    previous: Buffer,
    force_redraw: bool,
}

impl Renderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self { current: Buffer::new(width, height), previous: Buffer::new(width, height), force_redraw: true }
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.current
    }

    /// Write the last flushed frame to `path` as ANSI-escaped text.
    /// Each row is terminated with a newline so the file is `cat`-able.
    pub fn dump_previous(&self, path: &Path) -> io::Result<()> {
        let buf = &self.previous;
        let mut out = std::fs::File::create(path)?;

        for y in 0..buf.height {
            write!(out, "\x1b[0m")?;
            let mut last_fg: Option<Color> = None;
            let mut last_bg: Option<Color> = None;
            let mut last_bold = false;

            for x in 0..buf.width {
                let cell = buf.get(x, y);

                if last_fg != Some(cell.fg) {
                    let Color { r, g, b } = cell.fg;
                    write!(out, "\x1b[38;2;{r};{g};{b}m")?;
                    last_fg = Some(cell.fg);
                }
                if last_bg != Some(cell.bg) {
                    let Color { r, g, b } = cell.bg;
                    write!(out, "\x1b[48;2;{r};{g};{b}m")?;
                    last_bg = Some(cell.bg);
                }
                if cell.bold && !last_bold {
                    write!(out, "\x1b[1m")?;
                    last_bold = true;
                }

                let ch = if cell.ch.is_control() { ' ' } else { cell.ch };
                write!(out, "{ch}")?;
            }

            writeln!(out)?;
        }

        write!(out, "\x1b[0m")?;
        out.flush()
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.current.resize(width, height);
        self.previous.resize(width, height);
        self.force_redraw = true;
    }

    pub fn flush<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        let width = self.current.width;
        let height = self.current.height;

        let mut last_fg: Option<Color> = None;
        let mut last_bg: Option<Color> = None;
        let mut last_bold = false;
        let mut last_ul = UnderlineStyle::None;
        let mut last_ul_color: Option<Color> = None;

        for y in 0..height {
            for x in 0..width {
                let cur = self.current.get(x, y);
                let prev = self.previous.get(x, y);

                if !self.force_redraw && cur == prev {
                    continue;
                }

                write!(out, "\x1b[{};{}H", y + 1, x + 1)?;

                let ul_style_off = last_ul != UnderlineStyle::None && (cur.underline == UnderlineStyle::None || cur.underline != last_ul);
                let ul_color_off = last_ul_color.is_some() && cur.underline_color != last_ul_color;
                let needs_reset = (last_bold && !cur.bold) || ul_style_off || ul_color_off;

                if needs_reset {
                    write!(out, "\x1b[0m")?;
                    last_fg = None;
                    last_bg = None;
                    last_bold = false;
                    last_ul = UnderlineStyle::None;
                    last_ul_color = None;
                }

                if last_fg != Some(cur.fg) {
                    let Color { r, g, b } = cur.fg;
                    write!(out, "\x1b[38;2;{r};{g};{b}m")?;
                    last_fg = Some(cur.fg);
                }

                if last_bg != Some(cur.bg) {
                    let Color { r, g, b } = cur.bg;
                    write!(out, "\x1b[48;2;{r};{g};{b}m")?;
                    last_bg = Some(cur.bg);
                }

                if cur.bold && !last_bold {
                    write!(out, "\x1b[1m")?;
                    last_bold = true;
                }

                if cur.underline != UnderlineStyle::None && cur.underline != last_ul {
                    match cur.underline {
                        UnderlineStyle::Straight => write!(out, "\x1b[4m")?,
                        UnderlineStyle::Curly => write!(out, "\x1b[4:3m")?,
                        UnderlineStyle::None => {}
                    }
                    last_ul = cur.underline;
                }

                if cur.underline != UnderlineStyle::None && cur.underline_color != last_ul_color {
                    match cur.underline_color {
                        Some(Color { r, g, b }) => write!(out, "\x1b[58;2;{r};{g};{b}m")?,
                        None => write!(out, "\x1b[59m")?,
                    }
                    last_ul_color = cur.underline_color;
                }

                let ch = if cur.ch.is_control() { ' ' } else { cur.ch };
                write!(out, "{}", ch)?;
            }
        }

        write!(out, "\x1b[0m")?;
        out.flush()?;

        self.force_redraw = false;
        std::mem::swap(&mut self.current, &mut self.previous);
        self.current.clear();

        Ok(())
    }
}
