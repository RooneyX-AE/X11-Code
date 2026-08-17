use x11_agent::{swarm_event_bus::SwarmEventBus, swarm_events::SwarmEvent, swarm_view::SwarmView};

pub struct LiveSwarmBridge {
    receiver: tokio::sync::broadcast::Receiver<SwarmEvent>,
    pub view: SwarmView,
}

impl LiveSwarmBridge {
    pub fn subscribe(bus: &SwarmEventBus) -> Self {
        Self { receiver: bus.subscribe(), view: SwarmView::default() }
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
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    // The view is derived state. A lag means intermediate events were lost;
                    // callers should refresh/replay persisted swarm state before trusting it.
                    break;
                }
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
}
