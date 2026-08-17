use anyhow::Result;
use clap::{Parser, Subcommand};
use x11_agent::Agent;

#[derive(Debug, Parser)]
#[command(name = "x11", version, about = "X11 Code autonomous coding agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run { goal: String },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run { goal }) => {
            let mut agent = Agent::new(goal);
            agent.start()?;
            println!("X11 Code agent started: {:?}", agent.snapshot().state);
            agent.complete();
            println!("X11 Code agent finished: {:?}", agent.snapshot().state);
        }
        None => println!("X11 Code. Use `x11 run <goal>` to start an agent task."),
    }

    Ok(())
}
