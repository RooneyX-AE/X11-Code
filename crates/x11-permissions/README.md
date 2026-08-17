# x11-permissions

`x11-permissions` is the authorization layer between agent intent and tool execution.

## Evaluation order

Rules are evaluated from newest to oldest. A rule matches only when its operation matches and its optional subject pattern matches. A non-matching patterned rule is ignored and evaluation falls back to the operation's base decision.

The default policy allows reads and asks for shell, filesystem writes, Git writes, and network operations.

`decide_for()` is the operation used for actual tool subjects such as shell commands, file paths, Git arguments, and MCP tool names. Pattern matching is deliberately conservative and does not treat a plain pattern as a substring wildcard.
