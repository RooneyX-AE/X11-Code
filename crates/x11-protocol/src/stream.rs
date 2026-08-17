use crate::AgentEvent;
use std::{collections::HashMap, sync::{Arc, Mutex}};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

pub const DEFAULT_EVENT_BUFFER: usize = 1024;

#[derive(Clone)]
pub struct EventBus { sender: broadcast::Sender<AgentEvent> }
impl Default for EventBus { fn default() -> Self { Self::new(DEFAULT_EVENT_BUFFER) } }
impl EventBus {
    pub fn new(capacity: usize) -> Self { let (sender, _) = broadcast::channel(capacity.max(16)); Self { sender } }
    pub fn emit(&self, event: AgentEvent) -> Result<usize, broadcast::error::SendError<AgentEvent>> { self.sender.send(event) }
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> { self.sender.subscribe() }
    pub fn sender(&self) -> broadcast::Sender<AgentEvent> { self.sender.clone() }
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest { pub call_id: Uuid, pub tool: String, pub reason: String }

#[derive(Clone)]
pub struct ApprovalBroker {
    requests: mpsc::Sender<ApprovalRequest>,
    pending: Arc<Mutex<HashMap<Uuid, oneshot::Sender<bool>>>>,
}
impl ApprovalBroker {
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<ApprovalRequest>) { let (requests, receiver) = mpsc::channel(capacity.max(8)); (Self { requests, pending: Arc::new(Mutex::new(HashMap::new())) }, receiver) }
    pub async fn request(&self, request: ApprovalRequest) -> anyhow::Result<bool> {
        let call_id = request.call_id;
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().map_err(|_| anyhow::anyhow!("approval state lock poisoned"))?;
            if pending.contains_key(&call_id) { anyhow::bail!("approval request already pending: {call_id}"); }
            pending.insert(call_id, tx);
        }
        if let Err(err) = self.requests.send(request).await {
            if let Ok(mut pending) = self.pending.lock() { pending.remove(&call_id); }
            return Err(anyhow::anyhow!("approval channel closed: {err}"));
        }
        Ok(rx.await.unwrap_or(false))
    }
    pub fn resolve(&self, call_id:Uuid, approved:bool)->bool { self.pending.lock().ok().and_then(|mut p|p.remove(&call_id)).map(|sender|sender.send(approved).is_ok()).unwrap_or(false) }
    pub fn pending_count(&self)->usize { self.pending.lock().map(|p|p.len()).unwrap_or(0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test] async fn subscribers_receive_events(){let bus=EventBus::new(8);let mut rx=bus.subscribe();bus.emit(AgentEvent::StateChanged{state:"executing".into()}).unwrap();match rx.recv().await.unwrap(){AgentEvent::StateChanged{state}=>assert_eq!(state,"executing"),other=>panic!("unexpected event: {other:?}")}}
    #[tokio::test] async fn approval_round_trip(){let(broker,mut requests)=ApprovalBroker::new(8);let call_id=Uuid::new_v4();let waiter={let broker=broker.clone();tokio::spawn(async move{broker.request(ApprovalRequest{call_id,tool:"shell".into(),reason:"test".into()}).await.unwrap()})};let request=requests.recv().await.unwrap();assert_eq!(request.call_id,call_id);assert!(broker.resolve(call_id,true));assert!(waiter.await.unwrap());assert_eq!(broker.pending_count(),0);}
    #[tokio::test] async fn duplicate_call_id_is_rejected_without_replacing_first_waiter(){let(broker,mut requests)=ApprovalBroker::new(8);let call_id=Uuid::new_v4();let first={let broker=broker.clone();tokio::spawn(async move{broker.request(ApprovalRequest{call_id,tool:"shell".into(),reason:"first".into()}).await.unwrap()})};let _=requests.recv().await.unwrap();let second=broker.request(ApprovalRequest{call_id,tool:"shell".into(),reason:"second".into()}).await;assert!(second.is_err());assert_eq!(broker.pending_count(),1);assert!(broker.resolve(call_id,true));assert!(first.await.unwrap());}
    #[tokio::test] async fn closed_request_channel_cleans_pending_state(){let(broker,requests)=ApprovalBroker::new(8);drop(requests);let result=broker.request(ApprovalRequest{call_id:Uuid::new_v4(),tool:"shell".into(),reason:"closed".into()}).await;assert!(result.is_err());assert_eq!(broker.pending_count(),0);}
}
