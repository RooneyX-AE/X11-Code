use serde_json::json;
use x11_context::Context;
use x11_model::{CompletionRequest, Message, MockProvider, ToolCall};

#[tokio::test]
async fn assistant_tool_result_round_trip_preserves_call_identity() {
    let call = ToolCall {
        id: "call-42".into(),
        name: "read_file".into(),
        arguments: json!({"path":"src/main.rs"}),
    };

    let mut context = Context::default();
    context.push("system", "You are X11 Code.");
    context.push("user", "Read the file.");
    context.push_assistant_tool_calls(vec![call.clone()]);
    context.push_tool_result(&call.id, "fn main() {}\n");

    let messages = context.to_messages();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[2].role, "assistant");
    assert_eq!(messages[2].tool_calls[0].id, call.id);
    assert_eq!(messages[2].tool_calls[0].function.name, call.name);
    assert_eq!(messages[3].role, "tool");
    assert_eq!(messages[3].tool_call_id.as_deref(), Some(call.id.as_str()));
}

#[tokio::test]
async fn completion_request_serializes_valid_tool_message_shape() {
    let tool = ToolCall { id: "call-7".into(), name: "read_file".into(), arguments: json!({"path":"Cargo.toml"}) };
    let request = CompletionRequest {
        model: "mock".into(),
        messages: vec![
            Message::system("system"),
            Message::user("inspect"),
            Message::assistant_with_tools("", &[tool]),
            Message::tool("call-7", "workspace contents"),
        ],
        tools: Vec::new(),
        temperature: Some(0.1),
        max_tokens: Some(256),
    };
    let json = serde_json::to_value(&request).unwrap();
    let messages = json["messages"].as_array().unwrap();
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(messages[2]["tool_calls"][0]["type"], "function");
    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[3]["tool_call_id"], "call-7");
}

#[tokio::test]
async fn mock_provider_accepts_tool_history() {
    let provider = MockProvider;
    let result = provider.complete(CompletionRequest {
        model: "mock".into(),
        messages: vec![Message::system("rules"), Message::user("done")],
        tools: Vec::new(),
        temperature: None,
        max_tokens: Some(128),
    }).await.unwrap();
    assert!(result.text.contains("done"));
}
