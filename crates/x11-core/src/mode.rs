use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMode { Normal, Plan, Auto, Review }
impl Default for AgentMode { fn default() -> Self { Self::Normal } }
impl AgentMode {
    pub fn allows_writes(self) -> bool { matches!(self, Self::Normal | Self::Auto) }
    pub fn allows_shell(self) -> bool { matches!(self, Self::Normal | Self::Plan | Self::Auto) }
    pub fn allows_network(self) -> bool { !matches!(self, Self::Review) }
    pub fn is_read_only(self) -> bool { matches!(self, Self::Plan | Self::Review) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem { pub id: u64, pub title: String, pub status: TodoStatus, #[serde(default)] pub depends_on: Vec<u64> }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStatus { Pending, InProgress, Completed, Blocked }
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TodoList { pub items: Vec<TodoItem>, next_id: u64 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoError { EmptyTitle, UnknownDependency(u64), UnknownItem(u64), InvalidStatusTransition }
impl std::fmt::Display for TodoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Self::EmptyTitle => write!(f, "todo title cannot be empty"), Self::UnknownDependency(id) => write!(f, "todo dependency does not exist: {id}"), Self::UnknownItem(id) => write!(f, "todo item does not exist: {id}"), Self::InvalidStatusTransition => write!(f, "invalid todo status transition") }
    }
}
impl std::error::Error for TodoError {}

impl TodoList {
    pub fn add(&mut self, title: impl Into<String>, depends_on: Vec<u64>) -> Result<u64, TodoError> {
        let title = title.into();
        if title.trim().is_empty() { return Err(TodoError::EmptyTitle); }
        for dep in &depends_on { if !self.items.iter().any(|x| x.id == *dep) { return Err(TodoError::UnknownDependency(*dep)); } }
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        self.items.push(TodoItem { id, title, status: TodoStatus::Pending, depends_on });
        Ok(id)
    }
    pub fn update(&mut self, id: u64, status: TodoStatus) -> Result<(), TodoError> {
        let item = self.items.iter_mut().find(|item| item.id == id).ok_or(TodoError::UnknownItem(id))?;
        if matches!((item.status, status), (TodoStatus::Completed, TodoStatus::Pending | TodoStatus::InProgress) | (TodoStatus::Blocked, TodoStatus::Completed)) { return Err(TodoError::InvalidStatusTransition); }
        item.status = status;
        Ok(())
    }
    pub fn runnable(&self) -> Vec<&TodoItem> {
        self.items.iter().filter(|item| item.status == TodoStatus::Pending && item.depends_on.iter().all(|dep| self.items.iter().any(|x| x.id == *dep && x.status == TodoStatus::Completed))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn modes_have_explicit_capability_contract(){assert!(!AgentMode::Plan.allows_writes());assert!(AgentMode::Plan.allows_shell());assert!(!AgentMode::Review.allows_shell());assert!(!AgentMode::Review.allows_network());assert!(AgentMode::Auto.allows_writes());}
    #[test] fn todo_dependencies_and_transitions_are_validated(){let mut t=TodoList::default();assert_eq!(t.add("missing",vec![99]),Err(TodoError::UnknownDependency(99)));let a=t.add("inspect",vec![]).unwrap();let b=t.add("implement",vec![a]).unwrap();assert_eq!(t.runnable()[0].id,a);t.update(a,TodoStatus::Completed).unwrap();assert_eq!(t.runnable()[0].id,b);assert_eq!(t.update(a,TodoStatus::Pending),Err(TodoError::InvalidStatusTransition));}
}
