use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Context {
    items: Vec<ContextItem>,
}

impl Context {
    pub fn push(&mut self, role: impl Into<String>, content: impl Into<String>) {
        self.items.push(ContextItem { role: role.into(), content: content.into() });
    }

    pub fn items(&self) -> &[ContextItem] { &self.items }

    pub fn compact(&mut self, max_items: usize) {
        if self.items.len() > max_items {
            let keep_from = self.items.len() - max_items;
            self.items.drain(0..keep_from);
        }
    }
}
