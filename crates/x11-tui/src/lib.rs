use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Attribute, Print, SetAttribute},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{collections::VecDeque, io::{self, Write}, time::Duration};
use x11_protocol::AgentEvent;

const MAX_LOG_LINES: usize = 500;

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub tool: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct TuiState {
    pub state: String,
    pub goal: String,
    pub logs: VecDeque<String>,
    pub agents: Vec<(String, String, String)>,
    pub approval: Option<ApprovalRequest>,
    pub todo: Vec<(String, String)>,
}

impl TuiState {
    pub fn apply(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::SessionStarted { .. } => self.state = "starting".into(),
            AgentEvent::StateChanged { state } => self.state = state.clone(),
            AgentEvent::AssistantDelta { text } => self.push_log(format!("assistant: {text}")),
            AgentEvent::ToolRequested { tool, .. } => self.push_log(format!("tool → {tool}")),
            AgentEvent::ToolCompleted { tool: _, success, output, .. } => {
                self.push_log(format!("tool {}: {}", if *success { "✓" } else { "✗" }, output));
            }
            AgentEvent::ApprovalRequested { tool, reason, .. } => {
                self.approval = Some(ApprovalRequest { tool: tool.clone(), reason: reason.clone() });
                self.push_log(format!("approval required: {tool}"));
            }
            AgentEvent::SubagentStarted { agent_id, role, .. } => {
                self.agents.push((agent_id.clone(), role.clone(), "running".into()));
            }
            AgentEvent::SubagentFinished { agent_id, success, .. } => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.0 == *agent_id) {
                    agent.2 = if *success { "done" } else { "failed" }.into();
                }
            }
            AgentEvent::TodoChanged { task_id, title, status } => {
                let key = task_id.to_string();
                if let Some(item) = self.todo.iter_mut().find(|(id, _)| *id == key) {
                    item.1 = format!("{title} [{status}]");
                } else {
                    self.todo.push((key, format!("{title} [{status}]")));
                }
            }
            AgentEvent::CheckpointCreated { id, note } => self.push_log(format!("checkpoint {}: {note}", id)),
            AgentEvent::Verification { passed, summary } => self.push_log(format!("verify {}: {summary}", if *passed { "✓" } else { "✗" })),
            AgentEvent::Error { message } => self.push_log(format!("error: {message}")),
            AgentEvent::SessionFinished { success } => self.state = if *success { "completed" } else { "failed" }.into(),
            AgentEvent::PlanCreated { steps } => self.push_log(format!("plan: {} steps", steps.len())),
        }
    }

    fn push_log(&mut self, line: String) {
        for part in line.lines() { self.logs.push_back(part.to_owned()); }
        while self.logs.len() > MAX_LOG_LINES { self.logs.pop_front(); }
    }
}

pub fn run<I>(mut events: I) -> anyhow::Result<()>
where
    I: Iterator<Item = AgentEvent>,
{
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Clear(ClearType::All))?;
    let result = run_loop(&mut stdout, &mut events);
    execute!(stdout, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}

fn run_loop<W, I>(out: &mut W, events: &mut I) -> anyhow::Result<()>
where
    W: Write,
    I: Iterator<Item = AgentEvent>,
{
    let mut state = TuiState::default();
    loop {
        if let Some(event) = events.next() { state.apply(&event); }
        draw(out, &state)?;
        if event::poll(Duration::from_millis(80))? {
            match event::read()? {
                Event::Key(KeyEvent { code: KeyCode::Char('q'), .. }) => break,
                Event::Key(KeyEvent { code: KeyCode::Char('c'), modifiers, .. }) if modifiers.contains(KeyModifiers::CONTROL) => break,
                Event::Key(KeyEvent { code: KeyCode::Char('y'), .. }) => state.approval = None,
                Event::Key(KeyEvent { code: KeyCode::Char('n'), .. }) => state.approval = None,
                _ => {}
            }
        }
        if state.state == "completed" || state.state == "failed" { draw(out, &state)?; break; }
    }
    Ok(())
}

fn draw<W: Write>(out: &mut W, state: &TuiState) -> anyhow::Result<()> {
    let (width, height) = terminal::size()?;
    execute!(out, MoveTo(0, 0), Clear(ClearType::All), SetAttribute(Attribute::Bold), Print(" X11 CODE"), SetAttribute(Attribute::Reset))?;
    execute!(out, MoveTo(width.saturating_sub(30), 0), Print(format!("state: {}", state.state)))?;

    let sidebar = width.min(28);
    execute!(out, MoveTo(0, 2), Print(" AGENTS"))?;
    for (i, (id, role, status)) in state.agents.iter().enumerate().take(height.saturating_sub(6) as usize) {
        execute!(out, MoveTo(0, 3 + i as u16), Print(format!(" {:<10} {:<9} {}", id, role, status)))?;
    }

    execute!(out, MoveTo(sidebar, 2), Print(" ACTIVITY"))?;
    let available = height.saturating_sub(6) as usize;
    for (i, line) in state.logs.iter().rev().take(available).rev().enumerate() {
        let max = width.saturating_sub(sidebar + 1) as usize;
        let text = if line.len() > max { format!("{}…", &line[..max.saturating_sub(1)]) } else { line.clone() };
        execute!(out, MoveTo(sidebar, 3 + i as u16), Print(text))?;
    }

    if let Some(approval) = &state.approval {
        execute!(out, MoveTo(1, height.saturating_sub(3)), SetAttribute(Attribute::Bold), Print(format!(" APPROVAL: {} | {} | [y]es [n]o", approval.tool, approval.reason)), SetAttribute(Attribute::Reset))?;
    }

    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn state_tracks_subagents_and_approvals() {
        let id = Uuid::new_v4();
        let mut state = TuiState::default();
        state.apply(&AgentEvent::SubagentStarted { agent_id: "coder".into(), role: "Implementer".into(), session_id: x11_protocol::SessionId(id) });
        state.apply(&AgentEvent::ApprovalRequested { call_id: id, tool: "shell".into(), reason: "permission".into() });
        assert_eq!(state.agents[0].2, "running");
        assert_eq!(state.approval.as_ref().unwrap().tool, "shell");
    }
}
