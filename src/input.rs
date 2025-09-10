use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, PartialEq)]
pub enum EditorCommand {
    // Movement
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,

    // Editing
    InsertChar(char),
    Backspace,
    Delete,

    // Control
    Quit,
    Unknown,
}

pub fn map_key(event: KeyEvent, mode:Mode, pending: &Pending) -> KeyMappingResult {
    match event.code {
        KeyCode::Left => EditorCommand::MoveLeft,
        KeyCode::Right => EditorCommand::MoveRight,
        KeyCode::Backspace => EditorCommand::Backspace,
        KeyCode::Delete => EditorCommand::Delete,
        KeyCode::Esc => EditorCommand::Quit,
        KeyCode::Char(c) => EditorCommand::InsertChar(c),
        KeyCode::Up => EditorCommand::MoveUp,
        KeyCode::Down => EditorCommand::MoveDown,
        KeyCode::Enter => EditorCommand::InsertChar('\n'),
        _ => EditorCommand::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    #[test]
    fn test_quit_key() {
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(map_key(key), EditorCommand::Quit);
    }

    #[test]
    fn test_insert_char() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(map_key(key), EditorCommand::InsertChar('a'));
    }
}
