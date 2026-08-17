pub mod mode;
pub mod orchestration;
pub mod task_graph;
pub mod verification;
pub mod verification_engine;

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState { Idle, Planning, Executing, Verifying, Completed, Failed, Cancelled }
impl AgentState { pub fn is_terminal(self) -> bool { matches!(self, Self::Completed | Self::Failed | Self::Cancelled) } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot { pub session_id: Uuid, pub state: AgentState, pub iteration: u32, pub max_iterations: u32, pub goal: String, pub last_error: Option<String> }
impl AgentSnapshot {
    pub fn new(goal: impl Into<String>, max_iterations: u32) -> Self { Self { session_id: Uuid::new_v4(), state: AgentState::Idle, iteration: 0, max_iterations: max_iterations.max(1), goal: goal.into(), last_error: None } }
    pub fn transition(&mut self, next: AgentState) -> Result<(), TransitionError> {
        if self.state.is_terminal() { return Err(TransitionError::Terminal { from: self.state, to: next }); }
        let valid = matches!((self.state,next),(AgentState::Idle,AgentState::Planning)|(AgentState::Planning,AgentState::Executing)|(AgentState::Executing,AgentState::Executing)|(AgentState::Executing,AgentState::Verifying)|(AgentState::Verifying,AgentState::Planning)|(AgentState::Verifying,AgentState::Completed)|(AgentState::Planning,AgentState::Failed)|(AgentState::Executing,AgentState::Failed)|(AgentState::Verifying,AgentState::Failed)|(_,AgentState::Cancelled));
        if valid { self.state=next; Ok(()) } else { Err(TransitionError::Invalid { from:self.state,to:next }) }
    }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)] pub enum TransitionError { Invalid{from:AgentState,to:AgentState}, Terminal{from:AgentState,to:AgentState} }
impl fmt::Display for TransitionError { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{match self{Self::Invalid{from,to}=>write!(f,"invalid agent transition: {from:?} -> {to:?}"),Self::Terminal{from,to}=>write!(f,"cannot transition terminal state {from:?} -> {to:?}")}} }
impl std::error::Error for TransitionError {}
#[cfg(test)] mod tests { use super::*; #[test] fn transition_graph_is_guarded(){let mut s=AgentSnapshot::new("x",3);assert!(s.transition(AgentState::Executing).is_err());s.transition(AgentState::Planning).unwrap();s.transition(AgentState::Executing).unwrap();s.transition(AgentState::Verifying).unwrap();s.transition(AgentState::Completed).unwrap();assert!(s.transition(AgentState::Planning).is_err());} }
