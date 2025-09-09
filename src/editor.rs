use crate::input::EditorCommand;

use crate::graphemes::{
    abs_char_to_line_gcol, line_gcol_to_abs_char, next_grapheme_abs_char, prev_grapheme_abs_char,
};
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub struct Editor {
    pub cursor_row: usize,
    pub cursor_gcol: usize,      // grapheme cluster column
    desired_gcol: Option<usize>, // for vertical moves
    pub text: Rope,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            cursor_row: 0,
            cursor_gcol: 0,
            desired_gcol: None,
            text: Rope::new(),
        }
    }

    #[inline]
    fn line_gcount(&self, row: usize) -> usize {
        let s = self.text.line(row).to_string();
        UnicodeSegmentation::graphemes(s.as_str(), true).count()
    }

    #[inline]
    fn abs_char_at_cursor(&self) -> usize {
        line_gcol_to_abs_char(&self.text, self.cursor_row, self.cursor_gcol)
    }

    #[inline]
    fn clamp_gcol_on_row(&self, row: usize, gcol: usize) -> usize {
        gcol.min(self.line_gcount(row))
    }

    #[inline]
    fn set_desired_gcol(&mut self) {
        self.desired_gcol = Some(self.cursor_gcol);
    }

    #[inline]
    fn set_cursor_from_abs_char(&mut self, abs_char: usize) {
        let (row, gcol) = abs_char_to_line_gcol(&self.text, abs_char);
        self.cursor_row = row;
        self.cursor_gcol = gcol;
    }

    #[inline]
    fn clear_desired_gcol(&mut self) {
        self.desired_gcol = None;
    }

    pub fn handle_command(&self, command: EditorCommand) -> Self {
        let mut new = self.clone();

        match command {
            // ── Horizontal, grapheme‑aware ────────────────────────────────────────────
            EditorCommand::MoveLeft => {
                let here = new.abs_char_at_cursor();
                let prev = prev_grapheme_abs_char(&new.text, here);
                new.set_cursor_from_abs_char(prev);
                new.clear_desired_gcol();
            }

            EditorCommand::MoveRight => {
                let here = new.abs_char_at_cursor();
                let next = next_grapheme_abs_char(&new.text, here);
                new.set_cursor_from_abs_char(next);
                new.clear_desired_gcol();
            }

            // ── Vertical, grapheme‑aware (keep desired_gcol like Vim) ────────────────
            EditorCommand::MoveUp => {
                if new.cursor_row > 0 {
                    // Remember desired col the first time we go vertical in a streak
                    new.set_desired_gcol();
                    new.cursor_row -= 1;
                    let tgt = new.desired_gcol.unwrap();
                    new.cursor_gcol = new.clamp_gcol_on_row(new.cursor_row, tgt);
                }
            }

            EditorCommand::MoveDown => {
                if new.cursor_row + 1 < new.text.len_lines() {
                    new.set_desired_gcol();
                    new.cursor_row += 1;
                    let tgt = new.desired_gcol.unwrap();
                    new.cursor_gcol = new.clamp_gcol_on_row(new.cursor_row, tgt);
                }
            }

            // ── Insert: cursor is grapheme‑based; edits happen at char indices ───────
            EditorCommand::InsertChar(c) => {
                let at = new.abs_char_at_cursor();
                if c == '\n' {
                    new.text.insert(at, "\n");
                    new.cursor_row += 1;
                    new.cursor_gcol = 0;
                    new.clear_desired_gcol();
                } else {
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    new.text.insert(at, s);

                    // Move one grapheme forward from the insertion point
                    let next = next_grapheme_abs_char(&new.text, at);
                    new.set_cursor_from_abs_char(next);
                    new.clear_desired_gcol();
                }
            }

            // ── Backspace: delete previous grapheme cluster ───────────────────────────
            EditorCommand::Backspace => {
                let here = new.abs_char_at_cursor();
                if here > 0 {
                    let prev = prev_grapheme_abs_char(&new.text, here);
                    new.text.remove(prev..here);
                    new.set_cursor_from_abs_char(prev);
                }
                new.clear_desired_gcol();
            }

            // ── Delete: delete next grapheme cluster ───────────────────────────
            EditorCommand::Delete => {
                let here = new.abs_char_at_cursor(); // absolute *char* index
                let end = new.text.len_chars();

                if here < end {
                    // Find the next grapheme boundary after `here`
                    let next = next_grapheme_abs_char(&new.text, here);

                    if next > here {
                        // Remove the *entire* next grapheme cluster (could be a newline, emoji ZWJ, etc.)
                        new.text.remove(here..next);
                    } else {
                        // Extremely defensive fallback (shouldn't happen with the helper):
                        // remove a single char to make progress.
                        let limit = (here + 1).min(end);
                        if limit > here {
                            new.text.remove(here..limit);
                        }
                    }

                    // Cursor stays at the same logical position (absolute char index = `here`),
                    // but row/gcol may have changed (e.g., if we deleted a newline). Recompute:
                    new.set_cursor_from_abs_char(here);
                }

                // Editing horizontally? Clear the vertical “desired column” target.
                new.clear_desired_gcol();
            }

            EditorCommand::Quit | EditorCommand::Unknown => {}
        }

        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::EditorCommand;

    #[test]
    fn test_insert_char() {
        let editor = Editor::new();
        let updated = editor.handle_command(EditorCommand::InsertChar('a'));

        assert_eq!(updated.text.line(0).to_string(), "a");
        assert_eq!(updated.cursor_gcol, 1);
        assert_eq!(updated.cursor_row, 0);
    }

    #[test]
    fn test_move_down_and_up() {
        let mut editor = Editor::new();
        editor = editor.handle_command(EditorCommand::InsertChar('a'));
        editor = editor.handle_command(EditorCommand::InsertChar('\n'));
        editor = editor.handle_command(EditorCommand::InsertChar('b'));

        // After typing "a\nb", we have two lines: "a\n" and "b"
        // MoveDown should keep us at last line (row 1)
        let down = editor.handle_command(EditorCommand::MoveDown);
        assert_eq!(down.cursor_row, 1);

        let up = down.handle_command(EditorCommand::MoveUp);
        assert_eq!(up.cursor_row, 0);
    }

    #[test]
    fn emoji_is_one_step() {
        // "a👨‍👩‍👧‍👦b" — family emoji is a single grapheme made of multiple scalars.
        let mut ed = Editor::new();
        for ch in "a👨‍👩‍👧‍👦b".chars() {
            ed = ed.handle_command(EditorCommand::InsertChar(ch));
        }

        // Move left once: should jump from after 'b' to start of 'b'
        ed = ed.handle_command(EditorCommand::MoveLeft);
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_gcol, 2); // a, [emoji], |b|

        // Move left once more: should skip whole emoji in one step
        ed = ed.handle_command(EditorCommand::MoveLeft);
        assert_eq!(ed.cursor_gcol, 1); // a, |[emoji], b
    }

    #[test]
    fn combining_mark_is_one_step() {
        // "e\u{0301}" = "é" precomposed via combining acute
        let mut ed = Editor::new();
        for ch in "e\u{0301}".chars() {
            ed = ed.handle_command(EditorCommand::InsertChar(ch));
        }
        assert_eq!(ed.cursor_gcol, 1); // one grapheme on the first line

        // Backspace should delete the whole grapheme
        ed = ed.handle_command(EditorCommand::Backspace);
        assert_eq!(ed.cursor_gcol, 0);
        assert_eq!(ed.text.line(0).to_string(), "");
    }
    #[test]
    fn backspace_clears_combining_grapheme_and_resets_col() {
        let mut ed = Editor::new();
        for ch in "e\u{0301}".chars() {
            // "é"
            ed = ed.handle_command(EditorCommand::InsertChar(ch));
        }
        // One grapheme on the line
        assert_eq!(ed.cursor_gcol, 1);

        // Backspace should delete the full grapheme and move to col 0
        ed = ed.handle_command(EditorCommand::Backspace);
        assert_eq!(ed.text.line(0).to_string(), "");
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_gcol, 0); // <-- previously failed
    }
}
