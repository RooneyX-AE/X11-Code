use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;
use x11_agent::{AgentConfig, AgentRuntime};
use x11_core::mode::AgentMode;
use x11_model::{MockProvider, OpenAiCompatible};
use x11_session::{store::SessionStore, Session};
use x11_tui::stream::run_stream;

#[derive(Debug, Parser)]
#[command(name="x11", version, about="X11 Code autonomous coding agent")]
struct Cli { #[command(subcommand)] command: Option<Command> }

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        goal: String,
        #[arg(short,long)] workspace: Option<PathBuf>,
        #[arg(long)] model: Option<String>,
        #[arg(long)] yes: bool,
        #[arg(long)] max_iterations: Option<u32>,
        #[arg(long)] session: Option<PathBuf>,
        #[arg(long="verify", value_name="COMMAND", action=clap::ArgAction::Append)] verify_commands: Vec<String>,
        #[arg(long)] hooks: bool,
        #[arg(long)] verification_timeout_ms: Option<u64>,
        #[arg(long, default_value="normal")] mode: String,
        #[arg(long)] tui: bool,
    },
    Sessions { #[command(subcommand)] command: SessionCommand },
}

#[derive(Debug, Subcommand)]
enum SessionCommand { List { #[arg(short,long)] workspace: Option<PathBuf> }, Show { id: Uuid, #[arg(short,long)] workspace: Option<PathBuf> }, Fork { id: Uuid, goal: String, #[arg(short,long)] workspace: Option<PathBuf> } }

fn parse_mode(mode: &str) -> Result<AgentMode> {
    Ok(match mode.to_ascii_lowercase().as_str() { "normal" => AgentMode::Normal, "plan" => AgentMode::Plan, "auto" => AgentMode::Auto, "review" => AgentMode::Review, other => anyhow::bail!("unknown mode: {other}; use normal, plan, auto, or review") })
}
fn store_for(workspace: PathBuf) -> SessionStore { SessionStore::new(workspace.join(".x11/sessions")) }

async fn run_with_provider<P: x11_model::ModelProvider + 'static>(goal: String, cfg: AgentConfig, provider: P, tui: bool) -> Result<()> {
    let mut agent = AgentRuntime::new(goal, cfg, provider);
    if tui {
        let approval_requests = agent.enable_interactive_approvals();
        let broker = agent.approvals.clone();
        let events = agent.subscribe();
        let handle = tokio::spawn(async move {
            let mut agent = agent;
            agent.run().await
        });
        let mut stdout = std::io::stdout();
        let user_command = run_stream(&mut stdout, events, broker, approval_requests).await?;
        if matches!(user_command, x11_tui::stream::UserCommand::Quit) && !handle.is_finished() { handle.abort(); }
        match handle.await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) if matches!(user_command, x11_tui::stream::UserCommand::Quit) => { if err.to_string().contains("cancel") { Ok(()) } else { Err(err) } }
            Ok(Err(err)) => Err(err),
            Err(err) if err.is_cancelled() => Ok(()),
            Err(err) => Err(err.into()),
        }
    } else {
        let result = agent.run().await;
        for e in &agent.session.events { println!("{:?}", e); }
        result.map(|text| { if !text.is_empty() { println!("\n{text}"); } })?;
        Ok(())
    }
}

#[tokio::main]
async fn main()->Result<()>{
    tracing_subscriber::fmt().with_env_filter("x11=info").init();
    match Cli::parse().command {
        Some(Command::Run{goal,workspace,model,yes,max_iterations,session,verify_commands,hooks,verification_timeout_ms,mode,tui})=>{
            let mut cfg=AgentConfig::default();
            cfg.workspace=workspace.unwrap_or(std::env::current_dir()?); cfg.mode=parse_mode(&mode)?;
            if let Some(m)=model{cfg.model=m;}; cfg.auto_approve=yes || matches!(cfg.mode,AgentMode::Auto); cfg.hooks_enabled=hooks;
            if let Some(n)=max_iterations{cfg.max_iterations=n.max(1);} if let Some(ms)=verification_timeout_ms{cfg.verification_timeout_ms=ms.clamp(100,600_000);} if !verify_commands.is_empty(){cfg.verification_commands=verify_commands;}
            cfg.session_path=session.or_else(||Some(Session::default_path(&cfg.workspace)));
            if let (Ok(key),Ok(base))=(std::env::var("X11_API_KEY"),std::env::var("X11_BASE_URL")) {
                run_with_provider(goal,cfg,OpenAiCompatible::new(base,key),tui).await?;
            } else {
                run_with_provider(goal,cfg,MockProvider,tui).await?;
            }
        }
        Some(Command::Sessions{command})=>{
            let workspace=match &command { SessionCommand::List{workspace}|SessionCommand::Show{workspace,..}|SessionCommand::Fork{workspace,..} => workspace.clone().unwrap_or(std::env::current_dir()?) }; let store=store_for(workspace);
            match command { SessionCommand::List{..}=>{for (id,updated,goal) in store.list().await?{println!("{id}  {updated}  {goal}");}}, SessionCommand::Show{id,..}=>{let s=store.load(id).await?;println!("{}\n{}\n{} events",s.id,s.goal,s.events.len());}, SessionCommand::Fork{id,goal,..}=>{let s=store.load(id).await?;let fork=s.fork(goal);let path=store.save(&fork).await?;println!("forked {} -> {}\n{}",s.id,fork.id,path.display());} }
        }
        None=>println!("X11 Code. Use `x11 run <goal> [--tui]` or `x11 sessions list`.")
    }
    Ok(())
}
