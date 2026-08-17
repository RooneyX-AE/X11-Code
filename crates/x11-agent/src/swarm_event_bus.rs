use tokio::sync::broadcast;

use crate::swarm_events::SwarmEvent;

#[derive(Clone)]
pub struct SwarmEventBus {
    sender: broadcast::Sender<SwarmEvent>,
}

impl Default for SwarmEventBus {
    fn default() -> Self { Self::new(256) }
}

impl SwarmEventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SwarmEvent> { self.sender.subscribe() }

    pub fn emit(&self, event: SwarmEvent) -> usize { self.sender.send(event).unwrap_or(0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm_events::{SwarmEvent, SwarmEventKind};
    use uuid::Uuid;

    #[tokio::test]
    async fn subscribers_receive_swarm_events() {
        let bus = SwarmEventBus::new(8);
        let mut rx = bus.subscribe();
        let id = Uuid::new_v4();
        bus.emit(SwarmEvent::new(id, SwarmEventKind::SwarmStarted));
        let event = rx.recv().await.unwrap();
        assert_eq!(event.swarm_id, id);
        assert_eq!(event.kind, SwarmEventKind::SwarmStarted);
    }
}
