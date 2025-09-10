use crate::input::EditorCommand;

use crate::graphemes::{
    abs_char_to_line_gcol, line_gcol_to_abs_char, next_grapheme_abs_char, prev_grapheme_abs_char,
};
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
enum EditorMode {
    Normal,
    Insert,
    // Visual,
    // Command,
}

#[derive(Clone)]
// For future use: e.g., pending multi-key commands
// Currently unused
struct Pending {
    count: Option<usize>,
    register: Option<char>,
    prefix: Vec<Key>,
}

impl Pending {
    fn clear(&mut self) {
        self.count = None;
        self.register = None;
        self.prefix.clear();
    }
}

#[derive(Clone)]
pub struct Editor {
    pub cursor_row: usize,
    pub cursor_gcol: usize,      // grapheme cluster column
    desired_gcol: Option<usize>, // for vertical moves
    pub text: Rope,
    caret_abs: usize,
    mode: EditorMode,
    pending: Pending,

    #[cfg(debug_assertions)]
    last_newline_bol: Option<(usize, usize)>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            cursor_row: 0,
            cursor_gcol: 0,
            desired_gcol: None,
            text: Rope::new(),
            caret_abs: 0,
            mode: EditorMode::Insert,
            pending: Pending {
                count: None,
                register: None,
                prefix: Vec::new(),
            },
            #[cfg(debug_assertions)]
            last_newline_bol: None,
        }
    }

    #[inline]
    fn line_gcount(&self, row: usize) -> usize {
        let s = self.text.line(row).to_string();
        UnicodeSegmentation::graphemes(s.as_str(), true).count()
    }

    #[inline]
    fn abs_char_at_cursor(&self) -> usize {
        self.caret_abs
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

    #[inline]
    fn sync_visual_from_caret(&mut self) {
        self.set_cursor_from_abs_char(self.caret_abs);
    }

    #[inline]
    fn sync_caret_from_visual(&mut self) {
        self.caret_abs = line_gcol_to_abs_char(&self.text, self.cursor_row, self.cursor_gcol);
    }

    pub fn handle_command(&self, command: EditorCommand) -> Self {
        let mut new = self.clone();

        #[cfg(debug_assertions)]
        {
            // Visual -> abs (what the next insert would compute from row/gcol)
            let from_visual_abs = line_gcol_to_abs_char(&new.text, new.cursor_row, new.cursor_gcol);
            // Single source of truth for insertion:
            let anchor_abs = new.abs_char_at_cursor(); // == caret_abs

            debug_assert_eq!(
                from_visual_abs, anchor_abs,
                "Drift at command entry: visual and insert anchor disagree"
            );
        }
        #[cfg(debug_assertions)]
        {
            if let Some((row_cookie, bol_cookie)) = new.last_newline_bol.take() {
                // Only check if we’re still on that line for the very next event
                if new.cursor_row == row_cookie {
                    let caret_b = new.text.char_to_byte(new.abs_char_at_cursor());
                    if caret_b > bol_cookie {
                        // Something inserted before the caret between Enter and this key.
                        let span = new.text.byte_slice(bol_cookie..caret_b).to_string();
                        panic!(
                            "Auto-insert before caret after newline: {:?}",
                            span.escape_debug().to_string()
                        );
                    }
                }
            }
        }
        match command {
            // ── Horizontal, grapheme‑aware ────────────────────────────────────────────
            EditorCommand::MoveLeft => {
                let here = new.caret_abs;
                let prev = prev_grapheme_abs_char(&new.text, here);
                new.caret_abs = prev;
                new.sync_visual_from_caret();
                new.set_cursor_from_abs_char(prev);
                new.clear_desired_gcol();
                trace(&new, "after move left");
            }

            EditorCommand::MoveRight => {
                let here = new.caret_abs;
                let next = next_grapheme_abs_char(&new.text, here);
                new.caret_abs = next;
                new.sync_visual_from_caret();
                new.clear_desired_gcol();
                trace(&new, "after move right");
            }

            // ── Vertical, grapheme‑aware (keep desired_gcol like Vim) ────────────────
            EditorCommand::MoveUp => {
                if new.cursor_row > 0 {
                    new.set_desired_gcol();
                    new.cursor_row -= 1;
                    let tgt = new.desired_gcol.unwrap();
                    new.cursor_gcol = new.clamp_gcol_on_row(new.cursor_row, tgt);
                    new.sync_caret_from_visual(); // NEW
                    trace(&new, "after move up");
                }
            }

            EditorCommand::MoveDown => {
                if new.cursor_row + 1 < new.text.len_lines() {
                    new.set_desired_gcol();
                    new.cursor_row += 1;
                    let tgt = new.desired_gcol.unwrap();
                    new.cursor_gcol = new.clamp_gcol_on_row(new.cursor_row, tgt);
                    new.sync_caret_from_visual(); // NEW
                    trace(&new, "after move down");
                }
            }

            // ── Insert: cursor is grapheme‑based; edits happen at char indices ───────
            EditorCommand::InsertChar(c) => {
                let at = new.caret_abs; // single truth

                if c == '\n' {
                    new.text.insert(at, "\n");
                    // Move caret to just after the newline
                    let next = next_grapheme_abs_char(&new.text, at);
                    new.caret_abs = next;
                    new.sync_visual_from_caret();

                    #[cfg(debug_assertions)]
                    {
                        let bol_b = new.text.line_to_byte(new.cursor_row);
                        new.last_newline_bol = Some((new.cursor_row, bol_b));
                    }

                    trace(&new, "after newline insert");
                    new.clear_desired_gcol();
                } else {
                    // inside EditorCommand::InsertChar(c), before inserting non-'\n'
                    #[cfg(debug_assertions)]
                    {
                        let at_abs = new.abs_char_at_cursor();
                        let at_b = new.text.char_to_byte(at_abs);
                        let row = new.cursor_row;
                        let bol_b = new.text.line_to_byte(row);
                        let col_dbg = at_b.saturating_sub(bol_b);
                        eprintln!(
                            "[INSERT {:?}] row={} gcol={} | at_abs={} (byte off in line = {})",
                            c, row, new.cursor_gcol, at_abs, col_dbg
                        );
                    }
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    new.text.insert(at, s);

                    let next = next_grapheme_abs_char(&new.text, at);
                    new.caret_abs = next;
                    new.sync_visual_from_caret();
                    trace(&new, "after char insert");
                    new.clear_desired_gcol();
                }
            }

            // ── Backspace: delete previous grapheme cluster ───────────────────────────
            EditorCommand::Backspace => {
                let here = new.caret_abs;
                if here > 0 {
                    let del = if new.text.char(here - 1) == '\n' {
                        if here >= 2 && new.text.char(here - 2) == '\r' {
                            Some((here - 2, here))
                        } else {
                            Some((here - 1, here))
                        }
                    } else if new.text.char(here - 1) == '\r' {
                        Some((here - 1, here))
                    } else {
                        None
                    };

                    if let Some((start, end)) = del {
                        new.text.remove(start..end);
                        new.caret_abs = start;
                    } else {
                        let prev = prev_grapheme_abs_char(&new.text, here);
                        new.text.remove(prev..here);
                        new.caret_abs = prev;
                    }

                    new.sync_visual_from_caret();
                    trace(&new, "after backspace");
                }
                new.clear_desired_gcol();
            }

            // ── Delete: delete next grapheme cluster ───────────────────────────
            EditorCommand::Delete => {
                let here = new.caret_abs;
                let len = new.text.len_chars();

                if here < len {
                    // 1) Prefer deleting an actual line break starting at caret.
                    let del = if new.text.char(here) == '\n' {
                        Some(1) // Unix line break
                    } else if new.text.char(here) == '\r' {
                        // Handle CRLF pair as a single break if present
                        if here + 1 < len && new.text.char(here + 1) == '\n' {
                            Some(2)
                        } else {
                            Some(1)
                        }
                    } else {
                        None
                    };

                    if let Some(n) = del {
                        new.text.remove(here..here + n);
                    } else {
                        // 2) Otherwise, delete the *next grapheme cluster* (normal Delete)
                        let next = next_grapheme_abs_char(&new.text, here);
                        if next > here {
                            new.text.remove(here..next);
                        } else if here + 1 <= len {
                            // ultra-defensive fallback
                            new.text.remove(here..here + 1);
                        }
                    }

                    // Caret stays at same absolute index
                    new.sync_visual_from_caret();
                    trace(&new, "after delete");
                }

                new.clear_desired_gcol();
            }
            EditorCommand::Quit | EditorCommand::Unknown => {}
        }

        new
    }
}

fn trace(editor: &Editor, tag: &str) {
    let at_chars_from_visual =
        line_gcol_to_abs_char(&editor.text, editor.cursor_row, editor.cursor_gcol);
    let at_bytes = editor.text.char_to_byte(editor.caret_abs);
    let sol_bytes = editor.text.line_to_byte(editor.cursor_row);
    eprintln!(
        "[{tag}] row={} gcol={} | caret_abs={} (bytes={}) | from_visual_abs={} | BOL_bytes={}",
        editor.cursor_row,
        editor.cursor_gcol,
        editor.caret_abs,
        at_bytes,
        at_chars_from_visual,
        sol_bytes
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::EditorCommand;

    fn type_str(mut ed: Editor, s: &str) -> Editor {
        for ch in s.chars() {
            ed = ed.handle_command(EditorCommand::InsertChar(ch));
        }
        ed
    }

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
        assert_eq!(ed.cursor_gcol, 0);
    }
    #[test]
    fn newline_moves_caret_to_bol_and_next_char_is_col0() {
        // Start: ""
        let mut ed = Editor::new();

        // Type "hello", move left twice to end up after 'l'
        ed = type_str(ed, "hello");
        ed = ed.handle_command(EditorCommand::MoveLeft); // after 'l'
        ed = ed.handle_command(EditorCommand::MoveLeft); // after second 'l'

        // Press Enter: caret_abs must move to start of the next line (col 0)
        ed = ed.handle_command(EditorCommand::InsertChar('\n'));

        // Assert visual & anchor agree on BOL
        assert_eq!(ed.cursor_gcol, 0, "visual gcol should be 0 after newline");
        let caret_byte = ed.text.char_to_byte(ed.abs_char_at_cursor());
        let bol_byte = ed.text.line_to_byte(ed.cursor_row);
        assert_eq!(
            caret_byte, bol_byte,
            "caret_abs must be at BOL after newline"
        );

        // Now type 'X' — it MUST appear at column 0 on the new line
        ed = ed.handle_command(EditorCommand::InsertChar('X'));

        let line = ed.text.line(ed.cursor_row).to_string();
        assert!(
            line.starts_with('X'),
            "expected 'X' at col 0, got line {:?}",
            line
        );
        assert_eq!(
            ed.cursor_gcol, 1,
            "cursor should advance to col 1 after typing 'X'"
        );
    }

    #[test]
    fn vertical_move_resyncs_caret_abs_then_inserts_there() {
        // Buffer: "aa\nbb\ncc"
        let mut ed = Editor::new();
        ed = type_str(ed, "aa\nbb\ncc");

        // Put caret at end of first line: row 0, gcol 2
        // (We are currently at end of buffer; move up twice, then right to clamp)
        ed = ed.handle_command(EditorCommand::MoveUp);
        ed = ed.handle_command(EditorCommand::MoveUp);

        // MoveDown once: should land at row 1, same gcol (min with line length)
        ed = ed.handle_command(EditorCommand::MoveDown);
        assert_eq!(ed.cursor_row, 1);

        // Type 'Z' — must go into line 1 at the current visual gcol
        let before = ed.text.line(ed.cursor_row).to_string();
        ed = ed.handle_command(EditorCommand::InsertChar('Z'));
        let after = ed.text.line(ed.cursor_row).to_string();
        assert_ne!(before, after, "line should change after insert");
        assert!(
            after.contains('Z'),
            "expected 'Z' inserted on the target line"
        );
    }

    #[test]
    fn backspace_across_newline_moves_to_prev_line_end() {
        // Make two lines: "abc\n"
        let mut ed = Editor::new();
        ed = type_str(ed, "abc\n");

        // Now at start of second (empty) line; Backspace should delete the '\n'
        // and move caret to end of "abc"
        ed = ed.handle_command(EditorCommand::Backspace);

        assert_eq!(ed.text.to_string(), "abc");
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_gcol, 3);

        // Also check the anchor is at EOL in bytes
        let caret_byte = ed.text.char_to_byte(ed.abs_char_at_cursor());
        let eol_byte = ed.text.line_to_byte(0) + ed.text.line(0).len_bytes();
        assert_eq!(
            caret_byte, eol_byte,
            "caret_abs should end up at EOL of previous line"
        );
    }

    #[test]
    fn emoji_is_single_grapheme_for_moves_and_backspace() {
        // "a👨‍👩‍👧‍👦b" — family emoji is one grapheme
        let mut ed = Editor::new();
        ed = type_str(ed, "a");
        for ch in "👨‍👩‍👧‍👦".chars() {
            ed = ed.handle_command(EditorCommand::InsertChar(ch));
        }
        ed = ed.handle_command(EditorCommand::InsertChar('b'));
        assert_eq!(ed.cursor_row, 0);

        // MoveLeft: b -> [emoji]
        ed = ed.handle_command(EditorCommand::MoveLeft);
        let (row, gcol) = (ed.cursor_row, ed.cursor_gcol);
        // MoveLeft again: [emoji] -> a (skip entire cluster)
        ed = ed.handle_command(EditorCommand::MoveLeft);
        assert_eq!(ed.cursor_row, row);
        assert_eq!(ed.cursor_gcol, gcol - 1, "emoji should count as one step");

        // MoveRight back onto emoji then Backspace once: removes the whole emoji
        ed = ed.handle_command(EditorCommand::MoveRight);
        let len_before = ed.text.len_chars();
        ed = ed.handle_command(EditorCommand::Backspace);
        let len_after = ed.text.len_chars();
        assert!(
            len_after < len_before,
            "one backspace should remove entire emoji cluster"
        );
    }

    #[test]
    fn delete_over_newline_joins_lines_without_moving_caret_abs() {
        // Build: "foo\nbar"
        let mut ed = Editor::new();
        for ch in "foo\nbar".chars() {
            ed = ed.handle_command(EditorCommand::InsertChar(ch));
        }
        // Caret is at end (after 'r'). Move left 4 times:
        // r -> a -> b -> (start of line 1) -> just before '\n'
        ed = ed.handle_command(EditorCommand::MoveLeft); // after 'a'
        ed = ed.handle_command(EditorCommand::MoveLeft); // after 'b'
        ed = ed.handle_command(EditorCommand::MoveLeft); // after '\n' (row 1, col 0)
        ed = ed.handle_command(EditorCommand::MoveLeft); // before '\n' (row 0, col 3)

        // Sanity: we are at EOL of first line
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_gcol, 3);

        // Delete should remove the newline and join lines.
        ed = ed.handle_command(EditorCommand::Delete);

        assert_eq!(ed.text.to_string(), "foobar");
        // Caret stays at the same absolute char position (now before the old 'b')
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_gcol, 3);
    }

    #[test]
    fn delete_at_eol_joins_unix() {
        let mut ed = Editor::new();
        for ch in "foo\nbar".chars() {
            ed = ed.handle_command(EditorCommand::InsertChar(ch));
        }
        // Move to just before '\n'
        ed = ed.handle_command(EditorCommand::MoveLeft); // 'a'
        ed = ed.handle_command(EditorCommand::MoveLeft); // 'b'
        ed = ed.handle_command(EditorCommand::MoveLeft); // at row1 col0 (after '\n')
        ed = ed.handle_command(EditorCommand::MoveLeft); // before '\n' (row0 col3)

        ed = ed.handle_command(EditorCommand::Delete);
        assert_eq!(ed.text.to_string(), "foobar");
        assert_eq!((ed.cursor_row, ed.cursor_gcol), (0, 3));
    }

    #[test]
    fn delete_at_eol_joins_crlf() {
        let mut ed = Editor::new();
        // simulate CRLF explicitly
        for ch in "foo\r\nbar".chars() {
            ed = ed.handle_command(EditorCommand::InsertChar(ch));
        }
        // go to before '\r'
        ed = ed.handle_command(EditorCommand::MoveLeft);
        ed = ed.handle_command(EditorCommand::MoveLeft);
        ed = ed.handle_command(EditorCommand::MoveLeft);
        ed = ed.handle_command(EditorCommand::MoveLeft);

        ed = ed.handle_command(EditorCommand::Delete);
        assert_eq!(ed.text.to_string(), "foobar");
        assert_eq!((ed.cursor_row, ed.cursor_gcol), (0, 3));
    }
}
