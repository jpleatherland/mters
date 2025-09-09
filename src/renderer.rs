use crate::editor::Editor;
use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, execute};
use std::io::{Result, Stdout, Write};

pub fn render(stdout: &mut Stdout, editor: &Editor) -> Result<()> {
    execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0))?;

    // Option A: use Ropey's line iterator
    for line in editor.text.lines() {
        // Lines include the trailing newline (if present), so use `write!`, not `writeln!`.
        write!(stdout, "{}", line)?; // RopeSlice implements Display, so `{}` works
    }

    // If you're still using a char/grapheme-based column without width calculations:
    execute!(
        stdout,
        cursor::MoveTo(editor.cursor_gcol as u16, editor.cursor_row as u16),
    )?;

    stdout.flush()?;
    Ok(())
}
