use anyhow::Result;
use crossterm::{event::{self, Event, KeyCode, KeyEvent, KeyModifiers}, execute, terminal::{self, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType}};
use std::io::{self, Write};
use std::time::Duration;
use tokio::sync::broadcast;
use x11_protocol::AgentEvent;
use crate::{draw_snapshot, TuiState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserCommand { Approve, Deny, Quit }

pub fn handle_key(event: Event) -> Option<UserCommand> {
    match event {
        Event::Key(KeyEvent { code: KeyCode::Char('y'), .. }) => Some(UserCommand::Approve),
        Event::Key(KeyEvent { code: KeyCode::Char('n'), .. }) => Some(UserCommand::Deny),
        Event::Key(KeyEvent { code: KeyCode::Char('q'), .. }) => Some(UserCommand::Quit),
        Event::Key(KeyEvent { code: KeyCode::Char('c'), modifiers, .. }) if modifiers.contains(KeyModifiers::CONTROL) => Some(UserCommand::Quit),
        _ => None,
    }
}

pub async fn run_stream<W: Write>(out: &mut W, mut receiver: broadcast::Receiver<AgentEvent>) -> Result<UserCommand> {
    terminal::enable_raw_mode()?;
    execute!(out, EnterAlternateScreen, Clear(ClearType::All))?;
    let result = async {
        let mut state = TuiState::default();
        loop {
            while let Ok(event) = receiver.try_recv() { state.apply(&event); }
            let (width, height) = terminal::size()?;
            draw_snapshot(out, &state, width, height)?;
            if event::poll(Duration::from_millis(50))? {
                if let Some(command) = handle_key(event::read()?) {
                    if matches!(command, UserCommand::Quit) { return Ok(command); }
                    if state.approval.is_some() { return Ok(command); }
                }
            }
            if matches!(state.state.as_str(), "completed" | "failed") {
                while let Ok(event) = receiver.try_recv() { state.apply(&event); }
                let (width, height) = terminal::size()?;
                draw_snapshot(out, &state, width, height)?;
                return Ok(UserCommand::Quit);
            }
            match receiver.recv().await {
                Ok(event) => state.apply(&event),
                Err(broadcast::error::RecvError::Lagged(skipped)) => state.push_log(format!("event stream lagged; skipped {skipped} events")),
                Err(broadcast::error::RecvError::Closed) => return Ok(UserCommand::Quit),
            }
        }
    }.await;
    execute!(out, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}

#[allow(dead_code)]
fn _flush<W: Write>(out: &mut W) -> io::Result<()> { out.flush() }
