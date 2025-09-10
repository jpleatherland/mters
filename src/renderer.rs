use crate::editor::Editor;
use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, execute};
use std::io::{Result, Stdout, Write};

pub fn render(stdout: &mut Stdout, editor: &Editor) -> Result<()> {
    execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0))?;

    for (row, line) in editor.text.lines().enumerate() {
        write!(stdout, "{}", line)?; // prints text + '\n' if present
        execute!(stdout, cursor::MoveTo(0, (row + 1) as u16))?; // reset x to 0 for next row
    }

    execute!(
        stdout,
        cursor::MoveTo(editor.cursor_gcol as u16, editor.cursor_row as u16),
    )?;
    stdout.flush()?;
    Ok(())
}
