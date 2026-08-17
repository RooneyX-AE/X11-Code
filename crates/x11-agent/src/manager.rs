use anyhow::{anyhow, Result};
use std::{collections::{BTreeMap, BTreeSet}, path::PathBuf, sync::Arc};
use tokio::{sync::{watch, Semaphore}, task::JoinSet, time::{timeout, Duration}};
use uuid::Uuid;
use x11_core::{SubagentRole, SubagentSpec};
use x11_model::ModelProvider;
use x11_permissions::Policy;
use crate::{swarm_event_bus::SwarmEventBus, swarm_events::{SwarmEvent, SwarmEventKind}, swarm_state::{ResultSnapshot, SwarmState, SwarmTaskStatus}, workspace_lock::WorkspaceLockManager, AgentConfig, AgentRuntime};
use crate::tool_executor::ToolExecutor;

#[derive(Debug, Clone)]
pub struct SubagentResult { pub id:String, pub role:SubagentRole, pub success:bool, pub output:String, pub session_id:uuid::Uuid, pub iterations:u32, pub files_changed:Vec<String>, pub verification:String }

#[derive(Debug, Clone)]
pub struct SwarmReport { pub swarm_id:Uuid, pub results:Vec<SubagentResult>, pub succeeded:usize, pub failed:usize, pub conflict_candidates:Vec<String> }
impl SwarmReport { pub fn all_success(&self)->bool{self.failed==0} pub fn summary(&self)->String{format!("swarm complete: {} succeeded, {} failed, {} conflict candidate(s)",self.succeeded,self.failed,self.conflict_candidates.len())} }

#[derive(Debug, Clone)]
pub struct AgentManagerConfig { pub max_concurrency:usize, pub timeout_ms:u64, pub inherited_policy:Option<Policy>, pub cancellation:Option<watch::Sender<bool>>, pub state_path:Option<PathBuf>, pub event_bus:Option<SwarmEventBus>, pub swarm_id:Option<Uuid> }
impl Default for AgentManagerConfig { fn default()->Self{Self{max_concurrency:4,timeout_ms:7_200_000,inherited_policy:None,cancellation:None,state_path:None,event_bus:None,swarm_id:None}} }

pub struct AgentManager<P:ModelProvider+'static>{provider:Arc<P>,workspace:PathBuf,base_config:AgentConfig,config:AgentManagerConfig,locks:WorkspaceLockManager}
impl<P:ModelProvider+'static> AgentManager<P>{
 pub fn new(provider:Arc<P>,workspace:PathBuf,base_config:AgentConfig,config:AgentManagerConfig)->Self{Self{provider,workspace,base_config,config,locks:WorkspaceLockManager::default()}}
 fn swarm_id(&self)->Uuid{self.config.swarm_id.unwrap_or(Uuid::nil())}
 fn emit(&self,event:SwarmEvent){if let Some(bus)=&self.config.event_bus{bus.emit(event);}}
 async fn run_batch(&self,specs:Vec<SubagentSpec>)->Result<Vec<SubagentResult>>{
  if specs.is_empty(){return Ok(Vec::new());} let total=specs.len();let limit=self.config.max_concurrency.max(1).min(total);let semaphore=Arc::new(Semaphore::new(limit));let mut jobs=JoinSet::new();
  for spec in specs{let permit=semaphore.clone().acquire_owned().await?;let provider=self.provider.clone();let workspace=self.workspace.clone();let mut cfg=self.base_config.clone();let timeout_ms=self.config.timeout_ms;let locks=self.locks.clone();let inherited_policy=self.config.inherited_policy.clone();let parent_cancel=self.config.cancellation.clone();let bus=self.config.event_bus.clone();let swarm_id=self.swarm_id();jobs.spawn(async move{
   let _permit=permit;
   if let Some(bus)=&bus{bus.emit(SwarmEvent::new(swarm_id,SwarmEventKind::TaskStarted).task(spec.id.clone()).state("running").progress(1));}
   cfg.max_iterations=spec.max_iterations.max(1);cfg.model=spec.model.clone();cfg.max_context_tokens=(spec.token_budget as usize).clamp(2_000,128_000);cfg.auto_approve=false;cfg.session_path=None;
   let scoped_workspace=match &spec.workspace_scope{Some(scope)=>{let rel=std::path::Path::new(scope);if rel.is_absolute()||rel.components().any(|c|matches!(c,std::path::Component::ParentDir)){return Err(anyhow!("invalid workspace_scope for {}",spec.id));}let path=workspace.join(rel);if !path.is_dir(){return Err(anyhow!("workspace_scope does not exist for {}: {}",spec.id,path.display()));}path},None=>workspace.clone()};cfg.workspace=scoped_workspace;
   let role_prompt=match spec.role{SubagentRole::Explorer=>"Explore the repository. Do not modify files.",SubagentRole::Planner=>"Create a concrete implementation and verification plan. Prefer read-only inspection.",SubagentRole::Implementer=>"Implement the requested work with narrow edits and verification.",SubagentRole::Reviewer=>"Review the repository state and identify correctness, security, and regression risks.",SubagentRole::Tester=>"Run targeted tests/checks and diagnose failures. Avoid unrelated edits."};
   let goal=format!("Role: {role_prompt}\nTask: {}\nTool budget: {} calls. Preferred tools: {}",spec.goal,spec.tool_budget,if spec.allowed_tools.is_empty(){"all registered tools".into()}else{spec.allowed_tools.iter().cloned().collect::<Vec<_>>().join(", ")});
   let mut runtime=AgentRuntime::new_shared(goal,cfg,provider);let session_id=runtime.snapshot.session_id;
   if let Some(policy)=inherited_policy { runtime.policy=policy; }
   if let Some(cancel)=parent_cancel { runtime.cancel=cancel; }
   runtime.executor=if spec.allowed_tools.is_empty(){ToolExecutor::with_locks(runtime.tools.clone(),runtime.config.workspace.clone(),locks)}else{ToolExecutor::with_locks_and_allowlist(runtime.tools.clone(),runtime.config.workspace.clone(),locks,spec.allowed_tools.clone())};
   let result=timeout(Duration::from_millis(timeout_ms.max(1)),runtime.run()).await;
   let files_changed=runtime.session.events.iter().filter_map(|event|match event{AgentEvent::ToolRequested{tool,input,..} if tool=="write_file"||tool=="edit_file"=>input["path"].as_str().map(str::to_owned),_=>None}).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
   let verification_ok=result.as_ref().is_ok_and(|r|r.is_ok());
   if let Some(bus)=&bus{bus.emit(SwarmEvent::new(swarm_id,SwarmEventKind::VerificationStarted).task(spec.id.clone()).agent(spec.id.clone()).state("verifying"));bus.emit(SwarmEvent::new(swarm_id,if verification_ok{SwarmEventKind::VerificationPassed}else{SwarmEventKind::VerificationFailed}).task(spec.id.clone()).agent(spec.id.clone()).progress(100).state(if verification_ok{"passed"}else{"failed"}).evidence(if verification_ok{"runtime verification passed"}else{"runtime verification failed"}));}
   match result{Ok(Ok(output))=>{if let Some(bus)=&bus{bus.emit(SwarmEvent::new(swarm_id,SwarmEventKind::TaskCompleted).task(spec.id.clone()).agent(spec.id.clone()).progress(100).state("completed").evidence(format!("files_changed={}",files_changed.len())));}Ok(SubagentResult{id:spec.id,role:spec.role,success:true,output,session_id,iterations:runtime.snapshot.iteration,files_changed,verification:"runtime verification passed".into()})},Ok(Err(err))=>{if let Some(bus)=&bus{bus.emit(SwarmEvent::new(swarm_id,SwarmEventKind::TaskFailed).task(spec.id.clone()).agent(spec.id.clone()).progress(100).state("failed").evidence(err.to_string()));}Ok(SubagentResult{id:spec.id,role:spec.role,success:false,output:err.to_string(),session_id,iterations:runtime.snapshot.iteration,files_changed,verification:"runtime verification failed".into()})},Err(_)=>{if let Some(bus)=&bus{bus.emit(SwarmEvent::new(swarm_id,SwarmEventKind::TaskFailed).task(spec.id.clone()).agent(spec.id.clone()).progress(100).state("timed_out").evidence(format!("timeout_ms={timeout_ms}")));}Ok(SubagentResult{id:spec.id,role:spec.role,success:false,output:format!("subagent timed out after {} ms",timeout_ms),session_id,iterations:runtime.snapshot.iteration,files_changed,verification:"timed out".into()})}}
  });}
  let mut results=Vec::with_capacity(total);while let Some(joined)=jobs.join_next().await{results.push(joined.map_err(|e|anyhow!("subagent task join failure: {e}"))??);}results.sort_by(|a,b|a.id.cmp(&b.id));Ok(results)
 }
 pub async fn run_parallel(&self,specs:Vec<SubagentSpec>)->Result<Vec<SubagentResult>>{Ok(self.run_report(specs).await?.results)}
 pub async fn run_report(&self,specs:Vec<SubagentSpec>)->Result<SwarmReport>{self.run_report_internal(specs,self.config.state_path.clone()).await}
 pub async fn run_report_resumable(&self,specs:Vec<SubagentSpec>,state_path:impl Into<PathBuf>)->Result<SwarmReport>{self.run_report_internal(specs,Some(state_path.into())).await}
 async fn run_report_internal(&self,specs:Vec<SubagentSpec>,state_path:Option<PathBuf>)->Result<SwarmReport>{
  let swarm_id=self.swarm_id(); self.emit(SwarmEvent::new(swarm_id,SwarmEventKind::SwarmStarted).state("running").progress(0));
  let task_ids=specs.iter().map(|s|s.id.clone()).collect::<Vec<_>>();
  let mut state=if let Some(path)=&state_path{match SwarmState::load(path).await{Ok(mut loaded)=>{for task in loaded.tasks.values_mut(){if matches!(task.status,SwarmTaskStatus::Running){task.status=SwarmTaskStatus::Pending;}}self.emit(SwarmEvent::new(swarm_id,SwarmEventKind::SwarmResumed).state("resumed"));loaded},Err(_)=>SwarmState::new("swarm",task_ids.clone())}}else{SwarmState::new("swarm",task_ids.clone())};
  let known=task_ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
  for id in state.tasks.keys(){if !known.contains(id.as_str()){anyhow::bail!("persisted swarm task '{}' is absent from supplied specs",id);}}
  let mut completed=state.tasks.iter().filter(|(_,t)|matches!(t.status,SwarmTaskStatus::Succeeded)).map(|(id,_)|id.clone()).collect::<BTreeSet<_>>();
  let mut pending: BTreeMap<String,SubagentSpec>=specs.into_iter().filter(|s|!completed.contains(&s.id)).map(|s|(s.id.clone(),s)).collect();
  if let Some(path)=&state_path{state.save_atomic(path).await?;}
  let mut results=Vec::new();
  while !pending.is_empty(){
   let mut ready=pending.values().filter(|s|s.dependencies.iter().all(|d|completed.contains(d))).cloned().collect::<Vec<_>>();
   if ready.is_empty(){let unresolved=pending.keys().cloned().collect::<Vec<_>>().join(", ");self.emit(SwarmEvent::new(swarm_id,SwarmEventKind::TaskBlocked).state("blocked").evidence(unresolved.clone()));anyhow::bail!("subagent dependency cycle or missing dependency among: {unresolved}");}
   ready.sort_by(|a,b|b.priority.cmp(&a.priority).then_with(||a.id.cmp(&b.id)));
   for spec in &ready{self.emit(SwarmEvent::new(swarm_id,SwarmEventKind::TaskQueued).task(spec.id.clone()).parent_task("root").state("queued").progress(0));}
   if let Some(path)=&state_path{for spec in &ready{let _=state.mark_running(&spec.id,uuid::Uuid::nil());}state.save_atomic(path).await?;}
   let batch=self.run_batch(ready).await?;
   for result in &batch{completed.insert(result.id.clone());pending.remove(&result.id);if let Some(path)=&state_path{let _=state.mark_finished(&result.id,ResultSnapshot{success:result.success,cancelled:false,output:Some(result.output.clone()),error:(!result.success).then(||result.output.clone()),files_changed:result.files_changed.clone()});state.save_atomic(path).await?;}}
   results.extend(batch);
   if self.is_cancelled(){for id in pending.keys(){self.emit(SwarmEvent::new(swarm_id,SwarmEventKind::TaskCancelled).task(id.clone()).state("cancelled"));}break;}
  }
  results.sort_by(|a,b|a.id.cmp(&b.id));
  let persisted_success=state.tasks.values().filter(|t|matches!(t.status,SwarmTaskStatus::Succeeded)).count();
  let failed=results.iter().filter(|r|!r.success).count();
  let succeeded=persisted_success;
  let mut paths:BTreeMap<String,Vec<String>>=BTreeMap::new();for result in &results{for path in &result.files_changed{paths.entry(path.clone()).or_default().push(result.id.clone());}}
  let conflict_candidates=paths.into_iter().filter_map(|(path,owners)|if owners.len()>1{Some(format!("{path} <- {}",owners.join(", ")))}else{None}).collect::<Vec<_>>();
  for conflict in &conflict_candidates{self.emit(SwarmEvent::new(swarm_id,SwarmEventKind::ConflictDetected).state("conflict").evidence(conflict.clone()));}
  self.emit(SwarmEvent::new(swarm_id,SwarmEventKind::SwarmCompleted).state(if failed==0{"completed"}else{"completed_with_failures"}).progress(100).evidence(format!("succeeded={succeeded},failed={failed}")));
  Ok(SwarmReport{swarm_id,results,succeeded,failed,conflict_candidates})
 }
 fn is_cancelled(&self)->bool{self.config.cancellation.as_ref().map(|tx|*tx.borrow()).unwrap_or(false)}
}

#[cfg(test)]
mod tests{use super::*;use x11_model::MockProvider;
 fn spec(id:&str,role:SubagentRole,priority:i32,deps:&[&str])->SubagentSpec{SubagentSpec{id:id.into(),role,goal:id.into(),max_iterations:1,model:"default".into(),token_budget:4_000,tool_budget:8,allowed_tools:BTreeSet::new(),dependencies:deps.iter().map(|d|d.to_string()).collect(),priority,workspace_scope:None}}
 #[tokio::test] async fn manager_runs_isolated_agents_in_parallel(){let manager=AgentManager::new(Arc::new(MockProvider),PathBuf::from("."),AgentConfig::default(),AgentManagerConfig{max_concurrency:2,..Default::default()});let results=manager.run_parallel(vec![spec("b",SubagentRole::Explorer,0,&[]),spec("a",SubagentRole::Tester,0,&[])]).await.unwrap();assert_eq!(results.len(),2);assert_eq!(results[0].id,"a");assert_eq!(results[1].id,"b");assert!(results.iter().all(|r|r.success));assert_ne!(results[0].session_id,results[1].session_id);}
 #[tokio::test] async fn dependencies_are_scheduled_in_batches(){let manager=AgentManager::new(Arc::new(MockProvider),PathBuf::from("."),AgentConfig{..Default::default()},AgentManagerConfig{max_concurrency:2,..Default::default()});let report=manager.run_report(vec![spec("review",SubagentRole::Reviewer,0,&["impl"]),spec("impl",SubagentRole::Implementer,1,&[])]).await.unwrap();assert_eq!(report.results.len(),2);assert_eq!(report.failed,0);}
 #[tokio::test] async fn missing_dependency_is_rejected(){let manager=AgentManager::new(Arc::new(MockProvider),PathBuf::from("."),AgentConfig::default(),AgentManagerConfig::default());let err=manager.run_parallel(vec![spec("a",SubagentRole::Explorer,0,&["missing"])]).await.unwrap_err();assert!(err.to_string().contains("dependency"));}
 #[tokio::test] async fn persisted_successes_are_skipped_on_resume(){let dir=std::env::temp_dir().join(format!("x11-swarm-resume-{}",uuid::Uuid::new_v4()));let path=dir.join("state.json");let mut state=SwarmState::new("swarm",vec!["a".into(),"b".into()]);state.mark_finished("a",ResultSnapshot{success:true,cancelled:false,output:Some("done".into()),error:None,files_changed:Vec::new()}).unwrap();state.save_atomic(&path).await.unwrap();let manager=AgentManager::new(Arc::new(MockProvider),PathBuf::from("."),AgentConfig::default(),AgentManagerConfig::default());let report=manager.run_report_resumable(vec![spec("a",SubagentRole::Explorer,0,&[]),spec("b",SubagentRole::Explorer,0,&[])],&path).await.unwrap();assert_eq!(report.succeeded,2);let _=tokio::fs::remove_dir_all(dir).await;}
 #[test] fn report_aggregates_counts(){let report=SwarmReport{swarm_id:Uuid::nil(),results:Vec::new(),succeeded:0,failed:0,conflict_candidates:Vec::new()};assert!(report.all_success());assert_eq!(report.summary(),"swarm complete: 0 succeeded, 0 failed, 0 conflict candidate(s)");}
 #[tokio::test] async fn emits_lifecycle_events(){let bus=SwarmEventBus::new(64);let mut rx=bus.subscribe();let id=Uuid::new_v4();let manager=AgentManager::new(Arc::new(MockProvider),PathBuf::from("."),AgentConfig::default(),AgentManagerConfig{event_bus:Some(bus.clone()),swarm_id:Some(id),..Default::default()});let _=manager.run_report(vec![spec("a",SubagentRole::Explorer,0,&[])]).await.unwrap();let mut kinds=Vec::new();while let Ok(event)=rx.try_recv(){kinds.push(event.kind);}assert!(kinds.contains(&SwarmEventKind::SwarmStarted));assert!(kinds.contains(&SwarmEventKind::TaskQueued));assert!(kinds.contains(&SwarmEventKind::TaskStarted));assert!(kinds.contains(&SwarmEventKind::TaskCompleted));assert!(kinds.contains(&SwarmEventKind::SwarmCompleted));}
}
