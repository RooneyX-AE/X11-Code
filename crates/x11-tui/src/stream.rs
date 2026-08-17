use anyhow::Result;
use crossterm::{event::{self, Event, KeyCode, KeyEvent, KeyModifiers}, execute, terminal::{self, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType}};
use std::io::{self, Write};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use x11_protocol::{stream::{ApprovalBroker, ApprovalRequest}, AgentEvent};
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

pub async fn run_stream<W: Write>(out: &mut W, mut receiver: broadcast::Receiver<AgentEvent>, broker: ApprovalBroker, mut approval_requests: mpsc::Receiver<ApprovalRequest>) -> Result<UserCommand> {
    terminal::enable_raw_mode()?;
    execute!(out, EnterAlternateScreen, Clear(ClearType::All))?;
    let result = async {
        let mut state = TuiState::default();
        loop {
            while let Ok(event) = receiver.try_recv() { state.apply(&event); }
            while let Ok(request) = approval_requests.try_recv() {
                state.notice = Some(format!("approval queued: {}", request.tool));
            }

            if event::poll(Duration::from_millis(0))? {
                if let Some(command) = handle_key(event::read()?) {
                    match command {
                        UserCommand::Quit => return Ok(command),
                        UserCommand::Approve | UserCommand::Deny => {
                            if let Some(request) = state.approval.clone() {
                                let approved = matches!(command, UserCommand::Approve);
                                if broker.resolve(request.call_id, approved) {
                                    state.approval = None;
                                } else {
                                    state.notice = Some("approval request is no longer pending".into());
                                }
                            }
                        }
                    }
                }
            }

            let (width, height) = terminal::size()?;
            draw_snapshot(out, &state, width, height)?;

            if matches!(state.state.as_str(), "completed" | "failed") {
                while let Ok(event) = receiver.try_recv() { state.apply(&event); }
                let (width, height) = terminal::size()?;
                draw_snapshot(out, &state, width, height)?;
                return Ok(UserCommand::Quit);
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }.await;
    let restore = execute!(out, LeaveAlternateScreen);
    let raw = terminal::disable_raw_mode();
    result.and(restore.map_err(Into::into)).and(raw.map_err(Into::into))
}

#[allow(dead_code)]
fn _flush<W: Write>(out: &mut W) -> io::Result<()> { out.flush() }
