use serde::{Deserialize, Serialize};
use x11_model::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem { pub role: String, pub content: String }

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Context { items: Vec<ContextItem> }

impl Context {
    pub fn push(&mut self, role: impl Into<String>, content: impl Into<String>) { self.items.push(ContextItem{role:role.into(),content:content.into()}); }
    pub fn items(&self)->&[ContextItem]{&self.items}
    pub fn estimated_tokens(&self)->usize { self.items.iter().map(|i|(i.role.len()+i.content.len()+3)/4).sum() }

    pub fn compact(&mut self, max_tokens: usize) {
        if self.items.len() <= 2 || self.estimated_tokens() <= max_tokens { return; }
        let protected = self.protected_prefix_len();
        while self.estimated_tokens() > max_tokens && self.items.len() > protected + 1 {
            self.items.remove(protected);
        }
    }

    fn protected_prefix_len(&self) -> usize {
        let mut protected=0usize;
        for item in &self.items {
            if protected < 2 && (item.role == "system" || (protected == 1 && item.role == "user")) { protected += 1; }
            else { break; }
        }
        protected
    }

    pub fn to_messages(&self)->Vec<Message>{self.items.iter().map(|i|Message{role:i.role.clone(),content:i.content.clone()}).collect()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_estimates_and_compacts_without_losing_goal(){
        let mut c=Context::default();
        c.push("system","critical rules");
        c.push("user","original goal: fix the bug");
        for i in 0..20{c.push("assistant",format!("historical message {i} {}","x".repeat(100)));}
        let before=c.items().len();
        c.compact(100);
        assert!(c.items().len()<before);
        assert_eq!(c.items()[0].content,"critical rules");
        assert_eq!(c.items()[1].content,"original goal: fix the bug");
    }
    #[test]
    fn small_context_is_unchanged(){
        let mut c=Context::default();c.push("system","rules");c.push("user","goal");let before=c.items().to_vec();c.compact(10_000);assert_eq!(c.items(),before.as_slice());
    }
}
