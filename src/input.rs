use crate::editor::{EditorMode, Pending};
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
    InsertNewline,
    DeleteLine { count: usize },
    MoveToStartOfFile,
    WordForward { count: usize },
    Backspace,
    Delete,

    // Control
    EnterInsertMode,
    EnterNormalMode,
    Quit,
}

#[derive(Debug, PartialEq)]
pub enum KeyMappingResult {
    Command(EditorCommand),
    UpdatePending,
    Noop,
}

pub fn map_key(event: KeyEvent, mode: EditorMode, pending: &mut Pending) -> KeyMappingResult {
    use EditorCommand as Cmd;
    use KeyCode::*;

    if event.code == KeyCode::Esc {
        pending.clear();
        return KeyMappingResult::Command(Cmd::Quit);
    }

    match mode {
        EditorMode::Insert => {
            if event.code == Esc {
                pending.clear();
                return KeyMappingResult::Command(Cmd::EnterNormalMode);
            }
            match event.code {
                KeyCode::Char(c) => KeyMappingResult::Command(Cmd::InsertChar(c)),
                KeyCode::Delete => KeyMappingResult::Command(Cmd::Delete),
                KeyCode::Up => KeyMappingResult::Command(Cmd::MoveUp),
                KeyCode::Down => KeyMappingResult::Command(Cmd::MoveDown),
                KeyCode::Enter => KeyMappingResult::Command(Cmd::InsertNewline),
                KeyCode::Left => KeyMappingResult::Command(Cmd::MoveLeft),
                KeyCode::Right => KeyMappingResult::Command(Cmd::MoveRight),
                KeyCode::Backspace => KeyMappingResult::Command(Cmd::Backspace),
                KeyCode::Esc => KeyMappingResult::Command(Cmd::EnterNormalMode),
                _ => KeyMappingResult::Noop,
            }
        }

        EditorMode::Normal => {
            if event.code == Esc {
                pending.clear();
                return KeyMappingResult::Command(Cmd::Quit);
            }
            // ---- Count accumulation (e.g., "12w", "3dd") ----
            if let Char(d) = event.code {
                if d.is_ascii_digit() {
                    // accumulate digits: None -> d, 3 -> 3d, etc.
                    let digit = d.to_digit(10).unwrap() as usize;
                    let cur = pending.count.unwrap_or(0);
                    pending.count = Some(cur.saturating_mul(10).saturating_add(digit));
                    return KeyMappingResult::UpdatePending;
                }
            }

            // ---- Handle two-key prefixes already started ----
            match (pending.prefix.as_slice(), event.code) {
                // 'd' then 'd' => DeleteLine {count}
                ([KeyCode::Char('d')], KeyCode::Char('d')) => {
                    let n = pending.take_count();
                    pending.clear();
                    return KeyMappingResult::Command(Cmd::DeleteLine { count: n });
                }
                // 'g' then 'g' => MoveToStartOfFile
                ([KeyCode::Char('g')], KeyCode::Char('g')) => {
                    pending.clear();
                    return KeyMappingResult::Command(Cmd::MoveToStartOfFile);
                }
                // Unknown second key after a prefix: drop the prefix and interpret fresh
                ([KeyCode::Char('d')], _) | ([KeyCode::Char('g')], _) => {
                    pending.clear();
                    // fall through and treat this key as a fresh mapping
                }
                _ => {}
            }

            // ---- Start new prefixes ----
            match event.code {
                KeyCode::Char('d') => {
                    pending.push(KeyCode::Char('d'));
                    return KeyMappingResult::UpdatePending;
                }
                KeyCode::Char('g') => {
                    pending.push(KeyCode::Char('g'));
                    return KeyMappingResult::UpdatePending;
                }
                _ => {}
            }

            // ---- Plain normal-mode mappings ----
            match (event.code, event.modifiers) {
                (KeyCode::Char('i'), _) => KeyMappingResult::Command(Cmd::EnterInsertMode),
                (KeyCode::Char('w'), _) => {
                    let n = pending.take_count();
                    KeyMappingResult::Command(Cmd::WordForward { count: n })
                }
                (Left, _) => KeyMappingResult::Command(Cmd::MoveLeft),
                (Right, _) => KeyMappingResult::Command(Cmd::MoveRight),
                (Up, _) => KeyMappingResult::Command(Cmd::MoveUp),
                (Down, _) => KeyMappingResult::Command(Cmd::MoveDown),
                (Backspace, _) => KeyMappingResult::Command(Cmd::Backspace),
                (Delete, _) => KeyMappingResult::Command(Cmd::Delete),
                (Enter, _) => KeyMappingResult::Noop, // many editors do nothing for Enter in Normal
                _ => KeyMappingResult::Noop,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    #[test]
    fn test_quit_key() {
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut pending = Pending {
            count: None,
            register: None,
            prefix: Vec::new(),
        };
        let out = map_key(key, EditorMode::Insert, &mut pending);
        assert_eq!(out, KeyMappingResult::Command(EditorCommand::Quit));
    }

    #[test]
    fn test_insert_char() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let mut pending = Pending {
            count: None,
            register: None,
            prefix: Vec::new(),
        };
        let out = map_key(key, EditorMode::Insert, &mut pending);
        assert_eq!(
            out,
            KeyMappingResult::Command(EditorCommand::InsertChar('a'))
        );
    }
}
