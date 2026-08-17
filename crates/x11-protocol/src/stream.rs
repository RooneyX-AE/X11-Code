use crate::AgentEvent;
use tokio::sync::broadcast;

pub const DEFAULT_EVENT_BUFFER: usize = 1024;

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AgentEvent>,
}

impl Default for EventBus {
    fn default() -> Self { Self::new(DEFAULT_EVENT_BUFFER) }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(16));
        Self { sender }
    }

    pub fn emit(&self, event: AgentEvent) -> Result<usize, broadcast::error::SendError<AgentEvent>> {
        self.sender.send(event)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> { self.sender.subscribe() }
    pub fn sender(&self) -> broadcast::Sender<AgentEvent> { self.sender.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn subscribers_receive_events() {
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        bus.emit(AgentEvent::StateChanged { state: "executing".into() }).unwrap();
        match rx.recv().await.unwrap() {
            AgentEvent::StateChanged { state } => assert_eq!(state, "executing"),
            other => panic!("unexpected event: {other:?}"),
        }
        bus.emit(AgentEvent::CheckpointCreated { id: Uuid::new_v4(), note: "x".into() }).unwrap();
    }
}
