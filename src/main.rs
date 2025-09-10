use anyhow::Result;
use crossterm::{
    event::{self, Event},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::stdout;
use std::time::Duration;

mod editor;
mod graphemes;
mod input;
mod renderer;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    let mut editor = editor::Editor::new();

    loop {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key_event) = event::read()? {
                let kmr = input::map_key(key_event, editor.mode(), editor.pending_mut());

                match kmr {
                    input::KeyMappingResult::Command(cmd) => {
                        if let input::EditorCommand::Quit = cmd {
                            break;
                        }
                        editor = editor.handle_command(cmd);
                        renderer::render(&mut stdout, &editor)?;
                    }
                    input::KeyMappingResult::UpdatePending => {
                        // optional: render a “waiting for second key…” UI
                    }
                    input::KeyMappingResult::Noop => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    Ok(())
}
