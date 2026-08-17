use std::{collections::HashMap, sync::{Arc, Mutex}};
use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;
use x11_protocol::AgentEvent;

pub const EVENT_BUFFER: usize = 512;

#[derive(Clone)]
pub struct AgentControl {
    events: broadcast::Sender<AgentEvent>,
    approvals: Arc<Mutex<HashMap<Uuid, oneshot::Sender<bool>>>>,
}

impl Default for AgentControl {
    fn default() -> Self {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Self { events, approvals: Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl AgentControl {
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> { self.events.subscribe() }

    pub fn publish(&self, event: AgentEvent) {
        let _ = self.events.send(event);
    }

    pub async fn request_approval(&self, call_id: Uuid) -> bool {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut approvals) = self.approvals.lock() {
            approvals.insert(call_id, tx);
        } else {
            return false;
        }
        rx.await.unwrap_or(false)
    }

    pub fn resolve_approval(&self, call_id: Uuid, approved: bool) -> bool {
        let sender = self.approvals.lock().ok().and_then(|mut approvals| approvals.remove(&call_id));
        if let Some(sender) = sender { sender.send(approved).is_ok() } else { false }
    }

    pub fn pending_approvals(&self) -> usize {
        self.approvals.lock().map(|v| v.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn approval_round_trip() {
        let control = AgentControl::default();
        let cloned = control.clone();
        let task = tokio::spawn(async move { cloned.request_approval(Uuid::new_v4()).await });
        sleep(Duration::from_millis(5)).await;
        assert_eq!(control.pending_approvals(), 1);
        let id = {
            let approvals = control.approvals.lock().unwrap();
            *approvals.keys().next().unwrap()
        };
        assert!(control.resolve_approval(id, true));
        assert!(task.await.unwrap());
    }
}
