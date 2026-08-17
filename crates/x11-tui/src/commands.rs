#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Plan,
    Review,
    Compact,
    Resume,
    Swarm,
    Agents,
    Skills,
    Mcp,
    Plugins,
    Quit,
    Help,
    Unknown(String),
}

pub fn parse(input: &str) -> Option<Command> {
    let value = input.trim();
    if value.is_empty() || !value.starts_with('/') { return None; }
    Some(match value.to_ascii_lowercase().as_str() {
        "/plan" => Command::Plan,
        "/review" => Command::Review,
        "/compact" => Command::Compact,
        "/resume" => Command::Resume,
        "/swarm" => Command::Swarm,
        "/agents" => Command::Agents,
        "/skills" => Command::Skills,
        "/mcp" => Command::Mcp,
        "/plugins" => Command::Plugins,
        "/quit" | "/exit" => Command::Quit,
        "/help" | "/?" => Command::Help,
        other => Command::Unknown(other.to_owned()),
    })
}

pub fn help_lines() -> &'static [&'static str] {
    &[
        "/plan      switch to planning mode",
        "/review    inspect without mutation",
        "/compact   request context compaction",
        "/resume    inspect or resume session",
        "/swarm     inspect live swarm state",
        "/agents    list active agents",
        "/skills    list loaded skills",
        "/mcp       inspect MCP tools/servers",
        "/plugins   inspect loaded plugins",
        "/quit      exit X11 Code",
        "/help      show commands",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_commands() {
        assert_eq!(parse("/plan"), Some(Command::Plan));
        assert_eq!(parse(" /review "), Some(Command::Review));
        assert_eq!(parse("/swarm"), Some(Command::Swarm));
        assert_eq!(parse("/agents"), Some(Command::Agents));
        assert_eq!(parse("/skills"), Some(Command::Skills));
        assert_eq!(parse("/mcp"), Some(Command::Mcp));
        assert_eq!(parse("/plugins"), Some(Command::Plugins));
        assert_eq!(parse("hello"), None);
        assert!(matches!(parse("/unknown"), Some(Command::Unknown(_))));
    }
}
