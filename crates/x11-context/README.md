# x11-context

Conversation state for X11 Code. The context layer owns message history, approximate token accounting, compaction, and conversion to the model wire format.

## Invariants

- The leading `system` and `user` messages are protected during compaction.
- Assistant tool calls and their following tool results are removed as one unit. Compaction must never leave a dangling tool result.
- Tool results are bounded before they enter model context.
- `to_messages()` preserves `assistant` tool calls and `tool_call_id` relationships.
- Token estimation is deliberately approximate and is used as a safety budget, not as a tokenizer replacement.
