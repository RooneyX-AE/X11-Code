use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
pub struct CancellationToken {
    sender: Arc<watch::Sender<bool>>,
    receiver: watch::Receiver<bool>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender: Arc::new(sender), receiver }
    }
}

impl CancellationToken {
    pub fn new() -> Self { Self::default() }
    pub fn cancel(&self) { let _ = self.sender.send(true); }
    pub fn is_cancelled(&self) -> bool { *self.receiver.borrow() }
    pub fn subscribe(&self) -> watch::Receiver<bool> { self.sender.subscribe() }
    pub async fn cancelled(&mut self) { if *self.receiver.borrow() { return; } let _ = self.receiver.changed().await; }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_propagates_to_children() {
        let token = CancellationToken::new();
        let child = token.subscribe();
        let mut child_token = CancellationToken { sender: token.sender.clone(), receiver: child };
        assert!(!child_token.is_cancelled());
        token.cancel();
        child_token.cancelled().await;
        assert!(child_token.is_cancelled());
    }
}
