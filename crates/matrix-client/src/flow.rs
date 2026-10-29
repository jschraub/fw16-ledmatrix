//! One in-flight UI frame. Stale acknowledgements cannot release a newer session.
use crate::{ErrorCode, Failure};
use tokio::sync::oneshot;

#[derive(Default)]
pub struct Acknowledgements {
    subscription: String,
    pending: Option<(u32, oneshot::Sender<()>)>,
}

impl Acknowledgements {
    pub fn replace(&mut self, subscription: String) {
        self.subscription = subscription;
        self.pending = None;
    }

    pub fn arm(
        &mut self,
        subscription: &str,
        sequence: u32,
    ) -> Result<oneshot::Receiver<()>, Failure> {
        if subscription != self.subscription || self.pending.is_some() {
            return Err(Failure::new(ErrorCode::SlowConsumer));
        }
        let (tx, rx) = oneshot::channel();
        self.pending = Some((sequence, tx));
        Ok(rx)
    }

    pub fn acknowledge(&mut self, subscription: &str, sequence: u32) {
        if self.subscription == subscription
            && self
                .pending
                .as_ref()
                .is_some_and(|(expected, _)| *expected == sequence)
            && let Some((_, tx)) = self.pending.take()
        {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bounds_pending_frames_and_rejects_stale_acknowledgements() {
        let mut flow = Acknowledgements::default();
        flow.replace("first".into());
        let mut first = flow.arm("first", 7).unwrap();
        assert!(flow.arm("first", 8).is_err());
        flow.acknowledge("first", 6);
        assert!(matches!(
            first.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        flow.replace("second".into());
        assert!(first.await.is_err());
        let mut next = flow.arm("second", 7).unwrap();
        flow.acknowledge("first", 7);
        assert!(matches!(
            next.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        flow.acknowledge("second", 7);
        next.await.unwrap();
        assert!(flow.arm("second", 8).is_ok());
    }
}
