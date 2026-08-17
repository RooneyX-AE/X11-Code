use anyhow::Result;
use clap::{Parser,Subcommand};
use std::path::PathBuf;
use x11_agent::{AgentConfig,AgentRuntime};
use x11_model::{MockProvider,OpenAiCompatible};

#[derive(Debug,Parser)]#[command(name="x11",version,about="X11 Code autonomous coding agent")]
struct Cli{#[command(subcommand)]command:Option<Command>}

#[derive(Debug,Subcommand)]
enum Command{
    Run{
        goal:String,
        #[arg(short,long)] workspace:Option<PathBuf>,
        #[arg(long)] model:Option<String>,
        #[arg(long)] yes:bool,
        #[arg(long)] max_iterations:Option<u32>,
        #[arg(long)] session:Option<PathBuf>,
    }
}

#[tokio::main]
async fn main()->Result<()>{
    tracing_subscriber::fmt().with_env_filter("x11=info").init();
    let cli=Cli::parse();
    match cli.command{
        Some(Command::Run{goal,workspace,model,yes,max_iterations,session})=>{
            let mut cfg=AgentConfig::default();
            cfg.workspace=workspace.unwrap_or(std::env::current_dir()?);
            if let Some(m)=model{cfg.model=m;}
            cfg.auto_approve=yes;
            if let Some(n)=max_iterations{cfg.max_iterations=n.max(1);}
            cfg.session_path=session.or_else(||Some(cfg.workspace.join(".x11/session.json")));
            if let (Ok(key),Ok(base))=(std::env::var("X11_API_KEY"),std::env::var("X11_BASE_URL")){
                let provider=OpenAiCompatible::new(base,key);
                let mut agent=AgentRuntime::new(goal,cfg,provider);
                let result=agent.run().await;
                for e in &agent.session.events{println!("{:?}",e);}
                result.map(|text|{if !text.is_empty(){println!("\n{text}")}})?;
            }else{
                let mut agent=AgentRuntime::new(goal,cfg,MockProvider);
                let result=agent.run().await;
                for e in &agent.session.events{println!("{:?}",e);}
                result?;
            }
        }
        None=>println!("X11 Code. Use `x11 run <goal>`; set X11_API_KEY and X11_BASE_URL for an OpenAI-compatible provider."),
    }
    Ok(())
}
