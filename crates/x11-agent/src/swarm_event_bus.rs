use std::{collections::HashMap, sync::{Arc, Mutex, OnceLock}};
use tokio::sync::broadcast;
use uuid::Uuid;

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

static RUNTIME_BUSES: OnceLock<Mutex<HashMap<Uuid, Arc<SwarmEventBus>>>> = OnceLock::new();

fn runtime_registry() -> &'static Mutex<HashMap<Uuid, Arc<SwarmEventBus>>> {
    RUNTIME_BUSES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the process-local swarm bus associated with an AgentRuntime session.
/// The bus is shared by the swarm adapter and TUI without forcing the large
/// AgentRuntime type to grow another public field.
pub fn runtime_bus(session_id: Uuid) -> Arc<SwarmEventBus> {
    let mut registry = runtime_registry().lock().expect("runtime bus registry poisoned");
    registry.entry(session_id).or_insert_with(|| Arc::new(SwarmEventBus::default())).clone()
}

pub fn remove_runtime_bus(session_id: Uuid) {
    if let Ok(mut registry) = runtime_registry().lock() {
        registry.remove(&session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm_events::{SwarmEvent, SwarmEventKind};

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

    #[test]
    fn runtime_bus_is_shared_per_session() {
        let id = Uuid::new_v4();
        let a = runtime_bus(id);
        let b = runtime_bus(id);
        assert!(Arc::ptr_eq(&a, &b));
        remove_runtime_bus(id);
        let c = runtime_bus(id);
        assert!(!Arc::ptr_eq(&a, &c));
        remove_runtime_bus(id);
    }
}
