use std::io::{self, Write};

use crate::buffer::Buffer;
use crate::cell::Color;

pub struct Renderer {
    current: Buffer,
    previous: Buffer,
    force_redraw: bool,
}

impl Renderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            current: Buffer::new(width, height),
            previous: Buffer::new(width, height),
            force_redraw: true,
        }
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.current
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
        let mut last_underline = false;

        for y in 0..height {
            for x in 0..width {
                let cur = self.current.get(x, y);
                let prev = self.previous.get(x, y);

                if !self.force_redraw && cur == prev {
                    continue;
                }

                // always move cursor absolutely to avoid drifting that can be caused by skipped
                // cells or abnormally wide glyphs shifting the implicit pos.
                write!(out, "\x1b[{};{}H", y + 1, x + 1)?;

                // Reset if any attribute needs to turn off
                let needs_reset = (last_bold && !cur.bold) || (last_underline && !cur.underline);
                if needs_reset {
                    write!(out, "\x1b[0m")?;
                    last_fg = None;
                    last_bg = None;
                    last_bold = false;
                    last_underline = false;
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

                if cur.underline && !last_underline {
                    write!(out, "\x1b[4m")?;
                    last_underline = true;
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
