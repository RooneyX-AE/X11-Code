use anyhow::{Context, Result};
use std::{path::PathBuf, time::Duration};
use tokio::{process::Command, time::timeout};
use x11_permissions::{Decision, Operation, Policy};
use crate::{security::validate_hook_command, PluginHook};

const MAX_HOOK_OUTPUT: usize = 32_000;

#[derive(Debug, Clone)]
pub struct HookExecutionResult { pub success:bool, pub output:String, pub timed_out:bool }

fn truncate(mut text:String)->String{if text.len()<=MAX_HOOK_OUTPUT{return text;}text.truncate(MAX_HOOK_OUTPUT);text.push_str("\n...[plugin hook output truncated]...");text}

pub async fn execute_hook(plugin_root:PathBuf,hook:&PluginHook,policy:&Policy)->Result<HookExecutionResult>{
    let plugin=crate::Plugin{root:plugin_root.clone(),manifest:crate::PluginManifest::default()};
    validate_hook_command(&plugin,&hook.command)?;
    match policy.decide_for(Operation::Shell,&hook.command){Decision::Allow=>{},Decision::Deny=>anyhow::bail!("plugin hook denied by host policy"),Decision::Ask=>anyhow::bail!("plugin hook requires explicit approval")}
    let timeout_ms=hook.timeout_seconds.unwrap_or(30).clamp(1,120)*1000;
    let mut command=if cfg!(windows){Command::new("cmd")}else{Command::new("sh")};
    command.args(if cfg!(windows){vec!["/C",hook.command.as_str()]}else{vec!["-lc",hook.command.as_str()]}).current_dir(&plugin_root);
    let output=match timeout(Duration::from_millis(timeout_ms),command.output()).await{Ok(result)=>result.context("plugin hook execution failed")?,Err(_)=>return Ok(HookExecutionResult{success:false,output:format!("plugin hook timed out after {} ms",timeout_ms),timed_out:true})};
    let text=truncate(format!("stdout:\n{}\nstderr:\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr)));
    Ok(HookExecutionResult{success:output.status.success(),output:text,timed_out:false})
}

#[cfg(test)]
mod tests{
 use super::*;
 fn hook(command:&str)->PluginHook{PluginHook{event:"test".into(),matcher:None,command:command.into(),timeout_seconds:Some(5)}}
 #[tokio::test]async fn deny_policy_blocks_execution(){let root=std::env::temp_dir().join(format!("x11-plugin-hook-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir_all(&root).await.unwrap();let mut policy=Policy::default();policy.shell=Decision::Deny;let result=execute_hook(root.clone(),&hook("echo denied"),&policy).await;assert!(result.is_err());let _=tokio::fs::remove_dir_all(root).await;}
 #[tokio::test]async fn ask_policy_does_not_auto_grant(){let root=std::env::temp_dir().join(format!("x11-plugin-hook-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir_all(&root).await.unwrap();let policy=Policy::default();let result=execute_hook(root.clone(),&hook("echo ask"),&policy).await;assert!(result.is_err());let _=tokio::fs::remove_dir_all(root).await;}
 #[tokio::test]async fn timeout_is_reported_without_losing_result_metadata(){let root=std::env::temp_dir().join(format!("x11-plugin-hook-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir_all(&root).await.unwrap();let mut policy=Policy::default();policy.shell=Decision::Allow;let mut h=hook(if cfg!(windows){"ping -n 5 127.0.0.1 > nul"}else{"sleep 2"});h.timeout_seconds=Some(1);let result=execute_hook(root.clone(),&h,&policy).await.unwrap();assert!(result.timed_out);assert!(!result.success);let _=tokio::fs::remove_dir_all(root).await;}
}