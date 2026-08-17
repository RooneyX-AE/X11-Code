use serde::{Deserialize, Serialize};
use x11_model::{Message, ToolCall};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Context { items: Vec<ContextItem> }

impl Context {
    pub fn push(&mut self, role: impl Into<String>, content: impl Into<String>) {
        self.items.push(ContextItem { role: role.into(), content: content.into(), tool_calls: Vec::new(), tool_call_id: None });
    }
    pub fn push_assistant_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        self.items.push(ContextItem { role: "assistant".into(), content: String::new(), tool_calls, tool_call_id: None });
    }
    pub fn push_tool_result(&mut self, tool_call_id: impl Into<String>, content: impl Into<String>) {
        self.items.push(ContextItem { role: "tool".into(), content: content.into(), tool_calls: Vec::new(), tool_call_id: Some(tool_call_id.into()) });
    }
    pub fn items(&self)->&[ContextItem]{&self.items}
    pub fn estimated_tokens(&self)->usize { self.items.iter().map(|i| { let calls=i.tool_calls.iter().map(|c|c.name.len()+c.arguments.to_string().len()+c.id.len()).sum::<usize>(); (i.role.len()+i.content.len()+calls+3)/4 }).sum() }
    pub fn compact(&mut self, max_tokens: usize) { if self.items.len()<=2||self.estimated_tokens()<=max_tokens{return;}let protected=self.protected_prefix_len();while self.estimated_tokens()>max_tokens&&self.items.len()>protected+1{self.items.remove(protected);}}
    fn protected_prefix_len(&self)->usize{let mut protected=0usize;for item in &self.items{if protected<2&&(item.role=="system"||(protected==1&&item.role=="user")){protected+=1;}else{break;}}protected}
    pub fn to_messages(&self)->Vec<Message>{self.items.iter().map(|i|match i.role.as_str(){"assistant" if !i.tool_calls.is_empty()=>Message::assistant_with_tools(i.content.clone(),&i.tool_calls),"tool"=>Message::tool(i.tool_call_id.clone().unwrap_or_default(),i.content.clone()),"system"=>Message::system(i.content.clone()),"user"=>Message::user(i.content.clone()),_=>Message::assistant(i.content.clone())}).collect()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_estimates_and_compacts_without_losing_goal(){let mut c=Context::default();c.push("system","critical rules");c.push("user","original goal: fix the bug");for i in 0..20{c.push("assistant",format!("historical message {i} {}","x".repeat(100)));}let before=c.items().len();c.compact(100);assert!(c.items().len()<before);assert_eq!(c.items()[0].content,"critical rules");assert_eq!(c.items()[1].content,"original goal: fix the bug");}
    #[test]
    fn tool_protocol_is_preserved(){let mut c=Context::default();c.push_assistant_tool_calls(vec![ToolCall{id:"call-1".into(),name:"read_file".into(),arguments:serde_json::json!({"path":"a"})}]);c.push_tool_result("call-1","file contents");let m=c.to_messages();assert_eq!(m[0].tool_calls[0].id,"call-1");assert_eq!(m[1].tool_call_id.as_deref(),Some("call-1"));}
}
