use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMode {
    Normal,
    Plan,
    Auto,
    Review,
}

impl Default for AgentMode {
    fn default() -> Self { Self::Normal }
}

impl AgentMode {
    pub fn allows_writes(self) -> bool {
        matches!(self, Self::Normal | Self::Auto)
    }

    pub fn allows_shell(self) -> bool {
        !matches!(self, Self::Review)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: u64,
    pub title: String,
    pub status: TodoStatus,
    #[serde(default)]
    pub depends_on: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStatus { Pending, InProgress, Completed, Blocked }

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TodoList { pub items: Vec<TodoItem>, next_id: u64 }

impl TodoList {
    pub fn add(&mut self, title: impl Into<String>, depends_on: Vec<u64>) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.items.push(TodoItem { id, title: title.into(), status: TodoStatus::Pending, depends_on });
        id
    }

    pub fn update(&mut self, id: u64, status: TodoStatus) -> bool {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) { item.status = status; true } else { false }
    }

    pub fn runnable(&self) -> Vec<&TodoItem> {
        self.items.iter().filter(|item| item.status == TodoStatus::Pending && item.depends_on.iter().all(|dep| self.items.iter().find(|x| x.id == *dep).map(|x| x.status == TodoStatus::Completed).unwrap_or(false))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn todo_dependencies_work() {
        let mut t = TodoList::default();
        let a = t.add("inspect", vec![]);
        let b = t.add("implement", vec![a]);
        assert_eq!(t.runnable()[0].id, a);
        t.update(a, TodoStatus::Completed);
        assert_eq!(t.runnable()[0].id, b);
    }
}
