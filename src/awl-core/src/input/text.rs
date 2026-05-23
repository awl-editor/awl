use termion::event::{Event, Key};

pub enum TextInputCmd {
    None,    // unrecognised key — caller should mark not-dirty
    Moved,   // cursor moved, value unchanged
    Changed, // value changed — caller may trigger search / redraw
}

pub struct TextInput {
    pub value: String,
    pub cursor: usize,      // char index
    pub all_selected: bool, // true = next edit replaces all; movement clears it
}

impl TextInput {
    pub fn new() -> Self {
        Self { value: String::new(), cursor: 0, all_selected: false }
    }

    pub fn select_all(&mut self) {
        self.cursor = self.value.chars().count();
        self.all_selected = true;
    }

    pub fn handle_event(&mut self, event: &Event) -> TextInputCmd {
        match event {
            Event::Key(Key::Char(ch)) if !ch.is_control() => {
                if self.all_selected {
                    self.value.clear();
                    self.cursor = 0;
                    self.all_selected = false;
                }
                self.insert(*ch);
                TextInputCmd::Changed
            }
            Event::Key(Key::Backspace) => {
                if self.all_selected {
                    self.value.clear();
                    self.cursor = 0;
                    self.all_selected = false;
                    return TextInputCmd::Changed;
                }
                if self.delete_back() { TextInputCmd::Changed } else { TextInputCmd::None }
            }
            Event::Key(Key::Delete) => {
                if self.all_selected {
                    self.value.clear();
                    self.cursor = 0;
                    self.all_selected = false;
                    return TextInputCmd::Changed;
                }
                if self.delete_forward() { TextInputCmd::Changed } else { TextInputCmd::None }
            }
            Event::Key(Key::Left) => {
                self.all_selected = false;
                self.move_left(1);
                TextInputCmd::Moved
            }
            Event::Key(Key::Right) => {
                self.all_selected = false;
                self.move_right(1);
                TextInputCmd::Moved
            }
            Event::Key(Key::Home) => {
                self.all_selected = false;
                self.cursor = 0;
                TextInputCmd::Moved
            }
            Event::Key(Key::End) => {
                self.all_selected = false;
                self.cursor = self.value.chars().count();
                TextInputCmd::Moved
            }
            Event::Key(Key::CtrlLeft) => {
                self.all_selected = false;
                self.word_left();
                TextInputCmd::Moved
            }
            Event::Key(Key::CtrlRight) => {
                self.all_selected = false;
                self.word_right();
                TextInputCmd::Moved
            }
            Event::Unsupported(bytes) => match bytes.as_slice() {
                // Ctrl+Backspace (various terminals send different sequences)
                b"\x7f" | b"\x08" | b"\x1b\x7f" => {
                    if self.all_selected {
                        self.value.clear();
                        self.cursor = 0;
                        self.all_selected = false;
                        return TextInputCmd::Changed;
                    }
                    if self.delete_word_back() { TextInputCmd::Changed } else { TextInputCmd::None }
                }
                // Ctrl+Left / Ctrl+Right fallback escape sequences
                b"\x1b[1;5D" => {
                    self.all_selected = false;
                    self.word_left();
                    TextInputCmd::Moved
                }
                b"\x1b[1;5C" => {
                    self.all_selected = false;
                    self.word_right();
                    TextInputCmd::Moved
                }
                _ => TextInputCmd::None,
            },
            _ => TextInputCmd::None,
        }
    }

    fn chars(&self) -> Vec<char> {
        self.value.chars().collect()
    }

    fn insert(&mut self, ch: char) {
        let mut chars = self.chars();
        chars.insert(self.cursor, ch);
        self.value = chars.into_iter().collect();
        self.cursor += 1;
    }

    fn delete_back(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let mut chars = self.chars();
        chars.remove(self.cursor - 1);
        self.value = chars.into_iter().collect();
        self.cursor -= 1;
        true
    }

    fn delete_forward(&mut self) -> bool {
        let chars = self.chars();
        if self.cursor >= chars.len() {
            return false;
        }
        let mut chars = chars;
        chars.remove(self.cursor);
        self.value = chars.into_iter().collect();
        true
    }

    fn delete_word_back(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let chars = self.chars();
        let mut pos = self.cursor;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        if pos == self.cursor {
            return false;
        }
        let new: String = chars[..pos].iter().chain(chars[self.cursor..].iter()).collect();
        self.value = new;
        self.cursor = pos;
        true
    }

    pub fn move_left(&mut self, n: usize) {
        self.cursor = self.cursor.saturating_sub(n);
    }

    pub fn move_right(&mut self, n: usize) {
        self.cursor = (self.cursor + n).min(self.value.chars().count());
    }

    fn word_left(&mut self) {
        let chars = self.chars();
        let mut pos = self.cursor;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        self.cursor = pos;
    }

    fn word_right(&mut self) {
        let chars = self.chars();
        let len = chars.len();
        let mut pos = self.cursor;
        while pos < len && !chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
        self.cursor = pos;
    }

    /// How many chars to skip when the cursor is past the visible area.
    pub fn display_skip(&self, max_visible: usize) -> usize {
        if self.cursor <= max_visible { 0 } else { self.cursor - max_visible }
    }
}
