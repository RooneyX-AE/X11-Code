use anyhow::{anyhow, Result};
use std::{collections::{BTreeMap, BTreeSet}, path::PathBuf, sync::Arc};
use tokio::{sync::Semaphore, task::JoinSet, time::{timeout, Duration}};
use x11_core::{SubagentRole, SubagentSpec};
use x11_model::ModelProvider;

use crate::{workspace_lock::WorkspaceLockManager, AgentConfig, AgentRuntime};
use crate::tool_executor::ToolExecutor;

#[derive(Debug, Clone)]
pub struct SubagentResult { pub id:String, pub role:SubagentRole, pub success:bool, pub output:String, pub session_id:uuid::Uuid, pub iterations:u32, pub files_changed:Vec<String>, pub verification:String }

#[derive(Debug, Clone)]
pub struct AgentManagerConfig { pub max_concurrency:usize, pub timeout_ms:u64 }
impl Default for AgentManagerConfig { fn default()->Self{Self{max_concurrency:4,timeout_ms:7_200_000}} }

pub struct AgentManager<P:ModelProvider+'static>{provider:Arc<P>,workspace:PathBuf,base_config:AgentConfig,config:AgentManagerConfig,locks:WorkspaceLockManager}
impl<P:ModelProvider+'static> AgentManager<P>{
 pub fn new(provider:Arc<P>,workspace:PathBuf,base_config:AgentConfig,config:AgentManagerConfig)->Self{Self{provider,workspace,base_config,config,locks:WorkspaceLockManager::default()}}

 async fn run_batch(&self,specs:Vec<SubagentSpec>)->Result<Vec<SubagentResult>>{
  if specs.is_empty(){return Ok(Vec::new());}
  let total=specs.len();let limit=self.config.max_concurrency.max(1).min(total);let semaphore=Arc::new(Semaphore::new(limit));let mut jobs=JoinSet::new();
  for spec in specs{
   let permit=semaphore.clone().acquire_owned().await?;let provider=self.provider.clone();let workspace=self.workspace.clone();let mut cfg=self.base_config.clone();let timeout_ms=self.config.timeout_ms;let locks=self.locks.clone();
   jobs.spawn(async move{
    let _permit=permit;cfg.max_iterations=spec.max_iterations.max(1);cfg.model=spec.model.clone();cfg.max_context_tokens=(spec.token_budget as usize).clamp(2_000,128_000);cfg.auto_approve=false;cfg.session_path=None;
    let scoped_workspace=match &spec.workspace_scope{Some(scope)=>{let rel=std::path::Path::new(scope);if rel.is_absolute()||rel.components().any(|c|matches!(c,std::path::Component::ParentDir)){return Err(anyhow!("invalid workspace_scope for {}",spec.id));}let path=workspace.join(rel);if !path.is_dir(){return Err(anyhow!("workspace_scope does not exist for {}: {}",spec.id,path.display()));}path},None=>workspace.clone()};
    cfg.workspace=scoped_workspace;
    let role_prompt=match spec.role{SubagentRole::Explorer=>"Explore the repository. Do not modify files.",SubagentRole::Planner=>"Create a concrete implementation and verification plan. Prefer read-only inspection.",SubagentRole::Implementer=>"Implement the requested work with narrow edits and verification.",SubagentRole::Reviewer=>"Review the repository state and identify correctness, security, and regression risks.",SubagentRole::Tester=>"Run targeted tests/checks and diagnose failures. Avoid unrelated edits."};
    let goal=format!("Role: {role_prompt}\nTask: {}\nTool budget: {} calls. Preferred tools: {}",spec.goal,spec.tool_budget,if spec.allowed_tools.is_empty(){"all registered tools".into()}else{spec.allowed_tools.iter().cloned().collect::<Vec<_>>().join(", ")});
    let mut runtime=AgentRuntime::new_shared(goal,cfg,provider);let session_id=runtime.snapshot.session_id;
    runtime.executor=if spec.allowed_tools.is_empty(){ToolExecutor::with_locks(runtime.tools.clone(),runtime.config.workspace.clone(),locks)}else{ToolExecutor::with_locks_and_allowlist(runtime.tools.clone(),runtime.config.workspace.clone(),locks,spec.allowed_tools.clone())};
    let result=timeout(Duration::from_millis(timeout_ms.max(1)),runtime.run()).await;
    match result{Ok(Ok(output))=>Ok(SubagentResult{id:spec.id,role:spec.role,success:true,output,session_id,iterations:runtime.snapshot.iteration,files_changed:Vec::new(),verification:"runtime verification passed".into()}),Ok(Err(err))=>Ok(SubagentResult{id:spec.id,role:spec.role,success:false,output:err.to_string(),session_id,iterations:runtime.snapshot.iteration,files_changed:Vec::new(),verification:"runtime verification failed".into()}),Err(_)=>Ok(SubagentResult{id:spec.id,role:spec.role,success:false,output:format!("subagent timed out after {} ms",timeout_ms),session_id,iterations:runtime.snapshot.iteration,files_changed:Vec::new(),verification:"timed out".into()})}
   });
  }
  let mut results=Vec::with_capacity(total);while let Some(joined)=jobs.join_next().await{results.push(joined.map_err(|e|anyhow!("subagent task join failure: {e}"))??);}results.sort_by(|a,b|a.id.cmp(&b.id));Ok(results)
 }

 pub async fn run_parallel(&self,specs:Vec<SubagentSpec>)->Result<Vec<SubagentResult>>{
  let mut pending: BTreeMap<String,SubagentSpec>=specs.into_iter().map(|s|(s.id.clone(),s)).collect();let mut completed=BTreeSet::new();let mut results=Vec::new();
  while !pending.is_empty(){
   let mut ready=pending.values().filter(|s|s.dependencies.iter().all(|d|completed.contains(d))).cloned().collect::<Vec<_>>();
   if ready.is_empty(){let unresolved=pending.keys().cloned().collect::<Vec<_>>().join(", ");anyhow::bail!("subagent dependency cycle or missing dependency among: {unresolved}");}
   ready.sort_by(|a,b|b.priority.cmp(&a.priority).then_with(||a.id.cmp(&b.id)));
   let batch=self.run_batch(ready.clone()).await?;
   for result in &batch{completed.insert(result.id.clone());pending.remove(&result.id);}results.extend(batch);
  }
  results.sort_by(|a,b|a.id.cmp(&b.id));Ok(results)
 }
}

#[cfg(test)]
mod tests{use super::*;use x11_model::MockProvider;
 fn spec(id:&str,role:SubagentRole,priority:i32,deps:&[&str])->SubagentSpec{SubagentSpec{id:id.into(),role,goal:id.into(),max_iterations:1,model:"default".into(),token_budget:4_000,tool_budget:8,allowed_tools:BTreeSet::new(),dependencies:deps.iter().map(|d|d.to_string()).collect(),priority,workspace_scope:None}}
 #[tokio::test] async fn manager_runs_isolated_agents_in_parallel(){let manager=AgentManager::new(Arc::new(MockProvider),PathBuf::from("."),AgentConfig::default(),AgentManagerConfig{max_concurrency:2,timeout_ms:5_000});let results=manager.run_parallel(vec![spec("b",SubagentRole::Explorer,0,&[]),spec("a",SubagentRole::Tester,0,&[])]).await.unwrap();assert_eq!(results.len(),2);assert_eq!(results[0].id,"a");assert_eq!(results[1].id,"b");assert!(results.iter().all(|r|r.success));assert_ne!(results[0].session_id,results[1].session_id);}
 #[tokio::test] async fn dependencies_are_scheduled_in_batches(){let manager=AgentManager::new(Arc::new(MockProvider),PathBuf::from("."),AgentConfig::default(),AgentManagerConfig{max_concurrency:2,timeout_ms:5_000});let results=manager.run_parallel(vec![spec("review",SubagentRole::Reviewer,0,&["impl"]),spec("impl",SubagentRole::Implementer,1,&[])]).await.unwrap();assert_eq!(results.len(),2);}
 #[tokio::test] async fn missing_dependency_is_rejected(){let manager=AgentManager::new(Arc::new(MockProvider),PathBuf::from("."),AgentConfig::default(),AgentManagerConfig::default());let err=manager.run_parallel(vec![spec("a",SubagentRole::Explorer,0,&["missing"])]).await.unwrap_err();assert!(err.to_string().contains("dependency"));}
}