use serde::{Deserialize, Serialize};
use x11_model::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem { pub role: String, pub content: String }
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Context { items: Vec<ContextItem> }
impl Context {
    pub fn push(&mut self, role: impl Into<String>, content: impl Into<String>) { self.items.push(ContextItem{role:role.into(),content:content.into()}); }
    pub fn items(&self)->&[ContextItem]{&self.items}
    pub fn estimated_tokens(&self)->usize { self.items.iter().map(|i| (i.role.len()+i.content.len()+3)/4).sum() }
    pub fn compact(&mut self, max_tokens: usize) { while self.estimated_tokens()>max_tokens && self.items.len()>2 { self.items.remove(1); } }
    pub fn to_messages(&self)->Vec<Message>{self.items.iter().map(|i|Message{role:i.role.clone(),content:i.content.clone()}).collect()}
}
#[cfg(test)] mod tests { use super::*; #[test] fn context_estimates_and_compacts(){let mut c=Context::default();c.push("system","rules");for i in 0..20{c.push("user",format!("message {i} "+"x".repeat(100).as_str()));}let before=c.items().len();c.compact(100);assert!(c.items().len()<before);assert_eq!(c.items()[0].role,"system");} }
