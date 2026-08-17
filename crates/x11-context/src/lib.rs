use serde::{Deserialize, Serialize};
use x11_model::{Message, ToolCall};

const MAX_TOOL_RESULT_CHARS: usize = 32_000;

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
    pub fn push(&mut self, role: impl Into<String>, content: impl Into<String>) { self.items.push(ContextItem { role: role.into(), content: content.into(), tool_calls: Vec::new(), tool_call_id: None }); }
    pub fn push_assistant_message(&mut self, content: impl Into<String>, tool_calls: Vec<ToolCall>) { self.items.push(ContextItem { role: "assistant".into(), content: content.into(), tool_calls, tool_call_id: None }); }
    pub fn push_assistant_tool_calls(&mut self, tool_calls: Vec<ToolCall>) { self.push_assistant_message("", tool_calls); }
    pub fn push_tool_result(&mut self, tool_call_id: impl Into<String>, content: impl Into<String>) {
        let mut content = content.into();
        if content.chars().count() > MAX_TOOL_RESULT_CHARS { content = content.chars().take(MAX_TOOL_RESULT_CHARS).collect(); content.push_str("\n[tool output truncated by context limit]"); }
        self.items.push(ContextItem { role: "tool".into(), content, tool_calls: Vec::new(), tool_call_id: Some(tool_call_id.into()) });
    }
    pub fn items(&self) -> &[ContextItem] { &self.items }
    pub fn estimated_tokens(&self) -> usize { self.items.iter().map(|i| { let calls=i.tool_calls.iter().map(|c|c.name.len()+c.arguments.to_string().len()+c.id.len()).sum::<usize>(); (i.role.len()+i.content.len()+calls+3)/4 }).sum() }
    pub fn compact(&mut self, max_tokens: usize) {
        let target=max_tokens.max(128); if self.items.len()<=2 || self.estimated_tokens()<=target{return;}
        let protected=self.protected_prefix_len();
        while self.estimated_tokens()>target && self.items.len()>protected {
            let Some(end)=self.conversation_unit_end(protected) else {break}; self.items.drain(protected..end);
        }
    }
    fn protected_prefix_len(&self) -> usize { let mut p=0usize; for item in &self.items { if p==0 && item.role=="system" {p+=1;} else if p==1 && item.role=="user" {p+=1;} else {break;} } p }
    fn conversation_unit_end(&self, start: usize) -> Option<usize> {
        let item=self.items.get(start)?;
        if item.role=="assistant" && !item.tool_calls.is_empty() { let mut end=start+1; while self.items.get(end).is_some_and(|x|x.role=="tool") {end+=1;} Some(end) } else { Some(start+1) }
    }
    pub fn to_messages(&self) -> Vec<Message> { self.items.iter().map(|i| match i.role.as_str() { "assistant" if !i.tool_calls.is_empty()=>Message::assistant_with_tools(i.content.clone(),&i.tool_calls), "tool"=>Message::tool(i.tool_call_id.clone().unwrap_or_default(),i.content.clone()), "system"=>Message::system(i.content.clone()), "user"=>Message::user(i.content.clone()), _=>Message::assistant(i.content.clone()) }).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_compacts_without_losing_goal(){let mut c=Context::default();c.push("system","critical rules");c.push("user","original goal: fix the bug");for i in 0..20{c.push("assistant",format!("historical message {i} {}","x".repeat(100)));}let before=c.items().len();c.compact(200);assert!(c.items().len()<before);assert_eq!(c.items()[0].content,"critical rules");assert_eq!(c.items()[1].content,"original goal: fix the bug");}
    #[test]
    fn compaction_never_splits_tool_exchange(){let mut c=Context::default();c.push("system","rules");c.push("user","goal");for _ in 0..8{c.push_assistant_tool_calls(vec![ToolCall{id:"call-1".into(),name:"read_file".into(),arguments:serde_json::json!({"path":"a"})}]);c.push_tool_result("call-1","file contents");}c.compact(200);let messages=c.to_messages();for pair in messages.windows(2){if !pair[0].tool_calls.is_empty(){assert_eq!(pair[1].role,"tool");assert_eq!(pair[1].tool_call_id.as_deref(),Some("call-1"));}}}
    #[test]
    fn assistant_text_and_tool_calls_stay_together(){let mut c=Context::default();c.push_assistant_message("I will inspect the file",vec![ToolCall{id:"call-1".into(),name:"read_file".into(),arguments:serde_json::json!({"path":"a"})}]);let m=c.to_messages();assert_eq!(m[0].content,"I will inspect the file");assert_eq!(m[0].tool_calls[0].id,"call-1");}
    #[test]
    fn tool_protocol_is_preserved(){let mut c=Context::default();c.push_assistant_tool_calls(vec![ToolCall{id:"call-1".into(),name:"read_file".into(),arguments:serde_json::json!({"path":"a"})}]);c.push_tool_result("call-1","file contents");let m=c.to_messages();assert_eq!(m[0].tool_calls[0].id,"call-1");assert_eq!(m[1].tool_call_id.as_deref(),Some("call-1"));}
    #[test]
    fn tool_results_are_bounded(){let mut c=Context::default();c.push_tool_result("call-1","x".repeat(MAX_TOOL_RESULT_CHARS+100));assert!(c.items()[0].content.contains("truncated"));}
}
