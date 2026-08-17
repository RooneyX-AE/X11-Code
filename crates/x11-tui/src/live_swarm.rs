use uuid::Uuid;
use x11_agent::{swarm_event_bus::{runtime_bus, SwarmEventBus}, swarm_events::SwarmEvent, swarm_view::SwarmView};

pub struct LiveSwarmBridge {
    receiver: tokio::sync::broadcast::Receiver<SwarmEvent>,
    pub view: SwarmView,
}

impl LiveSwarmBridge {
    pub fn subscribe(bus: &SwarmEventBus) -> Self {
        Self { receiver: bus.subscribe(), view: SwarmView::default() }
    }

    pub fn for_session(session_id: Uuid) -> Self {
        let bus = runtime_bus(session_id);
        Self::subscribe(&bus)
    }

    pub fn try_poll(&mut self) -> usize {
        let mut applied = 0;
        loop {
            match self.receiver.try_recv() {
                Ok(event) => {
                    self.view.apply(event);
                    applied += 1;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            }
        }
        applied
    }

    pub async fn recv_one(&mut self) -> Option<SwarmEvent> {
        match self.receiver.recv().await {
            Ok(event) => {
                self.view.apply(event.clone());
                Some(event)
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11_agent::swarm_events::{SwarmEvent, SwarmEventKind};
    use uuid::Uuid;

    #[test]
    fn bridge_consumes_events_into_view() {
        let bus = SwarmEventBus::new(8);
        let mut bridge = LiveSwarmBridge::subscribe(&bus);
        let id = Uuid::new_v4();
        bus.emit(SwarmEvent::new(id, SwarmEventKind::TaskStarted).task("t1").agent("a1").progress(25).state("running"));
        assert_eq!(bridge.try_poll(), 1);
        assert_eq!(bridge.view.tasks["t1"].progress, 25);
        assert_eq!(bridge.view.agents["a1"].state, "running");
    }

    #[test]
    fn bridge_can_bind_to_runtime_session() {
        let session_id = Uuid::new_v4();
        let bus = runtime_bus(session_id);
        let mut bridge = LiveSwarmBridge::for_session(session_id);
        bus.emit(SwarmEvent::new(session_id, SwarmEventKind::ConflictDetected).state("conflict"));
        assert_eq!(bridge.try_poll(), 1);
        assert_eq!(bridge.view.state, "conflict");
        x11_agent::swarm_event_bus::remove_runtime_bus(session_id);
    }
}
