# x11-plugin

Plugins can provide skills, agents, commands, MCP metadata, and hooks. Plugin metadata does not grant execution permission.

The host validates plugin-relative paths and canonical containment. Hook execution must pass the host `x11-permissions::Policy`; `Ask` is never auto-granted by the plugin layer. Hook output is bounded and timeout state is explicit.
