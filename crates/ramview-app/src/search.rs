use windows::Win32::Foundation::RECT;

/// State for the custom-drawn single-line search field (kept native-free so the
/// glass styling and text metrics stay consistent with the rest of the HUD).
#[derive(Default)]
pub struct SearchField {
    pub text: String,
    /// Caret as a byte offset into `text` (always on a char boundary).
    pub caret: usize,
    /// Selection anchor; `Some` when a range is selected.
    pub anchor: Option<usize>,
    pub composition: String,
    pub composing: bool,
    pub focused: bool,
    /// Horizontal scroll in pixels.
    pub scroll: i32,
    pub box_rect: Option<RECT>,
    pub caret_x: i32,
    pub pending_caret_x: Option<i32>,
}

impl SearchField {
    pub fn selection(&self) -> Option<(usize, usize)> {
        match self.anchor {
            Some(anchor) if anchor != self.caret => {
                if anchor < self.caret {
                    Some((anchor, self.caret))
                } else {
                    Some((self.caret, anchor))
                }
            }
            _ => None,
        }
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.len();
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(start, end)| &self.text[start..end])
    }

    pub fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.selection() {
            self.text.replace_range(start..end, "");
            self.caret = start;
            self.anchor = None;
            true
        } else {
            false
        }
    }

    pub fn insert(&mut self, value: &str) {
        self.delete_selection();
        self.text.insert_str(self.caret, value);
        self.caret += value.len();
        self.anchor = None;
    }

    pub fn backspace(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let previous = self.text[..self.caret]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.text.replace_range(previous..self.caret, "");
        self.caret = previous;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret >= self.text.len() {
            return false;
        }
        let next = self.text[self.caret..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.caret + index)
            .unwrap_or(self.text.len());
        self.text.replace_range(self.caret..next, "");
        true
    }

    pub fn move_caret(&mut self, forward: bool, extend: bool) {
        let anchor = if extend {
            self.anchor.or(Some(self.caret))
        } else {
            None
        };

        if forward {
            if let Some((index, character)) = self.text[self.caret..].char_indices().next() {
                self.caret += index + character.len_utf8();
            }
        } else if self.caret > 0 {
            self.caret = self.text[..self.caret]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
        }

        self.anchor = anchor;
    }

    pub fn home(&mut self, extend: bool) {
        let anchor = if extend {
            self.anchor.or(Some(self.caret))
        } else {
            None
        };
        self.caret = 0;
        self.anchor = anchor;
    }

    pub fn end(&mut self, extend: bool) {
        let anchor = if extend {
            self.anchor.or(Some(self.caret))
        } else {
            None
        };
        self.caret = self.text.len();
        self.anchor = anchor;
    }
}
