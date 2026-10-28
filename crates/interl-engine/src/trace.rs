//! Retained trace accounting and live delivery with cancellation-aware backpressure.
use super::*;
use std::time::Instant;

/// Owns the bounded report and delivers the same redacted events to adapters.
pub(crate) struct Trace {
    pub(crate) run_id: String,
    pub(crate) user: String,
    pub(crate) sender: mpsc::Sender<RunEvent>,
    pub(crate) cancel: CancellationToken,
    pub(crate) start: Instant,
    pub(crate) snapshot: Value,
    pub(crate) summary: RunSummary,
    pub(crate) events: Vec<RunEvent>,
    pub(crate) bytes: usize,
    pub(crate) limit: usize,
    pub(crate) truncated: bool,
    pub(crate) dropped: u64,
    pub(crate) context: EventContext,
    pub(crate) control: RunControl,
    pub(crate) cleanup_deadline: Option<tokio::time::Instant>,
    pub(crate) cleanup_expired: bool,
}
impl Trace {
    pub(crate) fn new(
        run_id: String,
        user: String,
        sender: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
        snapshot: Value,
        limit: usize,
    ) -> Self {
        let bytes = budget::value_bytes(&snapshot) + 1024;
        Self {
            run_id,
            user,
            sender,
            control: RunControl::from_token(cancel.clone()),
            context: EventContext::default(),
            cleanup_deadline: None,
            cleanup_expired: false,
            cancel,
            start: Instant::now(),
            snapshot,
            summary: RunSummary::default(),
            events: Vec::new(),
            bytes,
            limit,
            truncated: false,
            dropped: 0,
        }
    }
    pub(crate) fn elapsed(&self) -> u64 {
        self.start.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
    pub(crate) async fn emit(
        &mut self,
        node: Option<&str>,
        event: EventKind,
        terminal: bool,
    ) -> bool {
        if !terminal
            && self
                .cleanup_deadline
                .is_some_and(|d| tokio::time::Instant::now() >= d)
        {
            self.cleanup_expired = true;
            return false;
        }
        let event = RunEvent {
            run_id: self.run_id.clone(),
            sequence: self.events.len() as u64 + 1,
            elapsed_ns: self.elapsed(),
            user_id: Some(self.user.clone()),
            node_id: node.map(str::to_owned),
            event,
            context: self.context.clone(),
        };
        let Ok(bytes) = budget::serialized_bytes(&event) else {
            self.truncated = true;
            return false;
        };
        let bytes = bytes.saturating_add(1);
        if !terminal
            && (self.bytes + bytes > self.limit.saturating_sub(65536)
                || self.events.len() >= 2048 - 64)
        {
            self.truncated = true;
            return false;
        }
        self.bytes += bytes;
        self.events.push(event.clone());
        // The report owns one copy. Reserve channel space before moving the other
        // copy into live delivery: select! drops losing futures, so constructing a
        // send(event) future would consume the event even when cancellation wins.
        // Biased ordering gives deadlines and cancellation priority over delivery.
        let deadline = self.cleanup_deadline.filter(|_| !terminal);
        tokio::select! {
            biased;
            _ = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                self.dropped += 1;
                self.cleanup_expired = true;
                return false;
            },
            _ = self.cancel.cancelled() => {
                if let Err(error) = self.sender.try_send(event) {
                    self.dropped += 1;
                    if matches!(error, mpsc::error::TrySendError::Closed(_)) {
                        self.control.stop(StopCause::ReceiverClosed);
                    }
                }
            },
            result = self.sender.reserve() => match result {
                Ok(permit) => permit.send(event),
                Err(_) => {
                    self.dropped += 1;
                    self.control.stop(StopCause::ReceiverClosed);
                }
            },
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(sender: mpsc::Sender<RunEvent>, cancel: CancellationToken) -> Trace {
        Trace::new(
            "run".into(),
            "user".into(),
            sender,
            cancel,
            Value::Null,
            131_072,
        )
    }

    #[tokio::test]
    async fn cancellation_retains_terminal_events_when_live_channel_is_full() {
        let (sender, mut receiver) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let mut trace = trace(sender, cancel.clone());
        assert!(trace.emit(None, EventKind::UserSpawned {}, false).await);
        cancel.cancel();
        assert!(
            trace
                .emit(
                    None,
                    EventKind::RunFinished {
                        status: RunStatus::Cancelled
                    },
                    true
                )
                .await
        );
        assert_eq!(trace.events.len(), 2);
        assert_eq!(trace.dropped, 1);
        assert_eq!(receiver.recv().await.unwrap().sequence, 1);
    }

    #[tokio::test]
    async fn closed_receiver_retains_event_and_records_shutdown_cause() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        let mut trace = trace(sender, CancellationToken::new());
        assert!(trace.emit(None, EventKind::UserSpawned {}, false).await);
        assert_eq!(trace.events.len(), 1);
        assert_eq!(trace.dropped, 1);
        assert_eq!(trace.control.cause(), Some(StopCause::ReceiverClosed));
    }

    #[tokio::test]
    async fn expired_cleanup_deadline_preserves_terminal_delivery() {
        let (sender, mut receiver) = mpsc::channel(1);
        let mut trace = trace(sender, CancellationToken::new());
        trace.cleanup_deadline = Some(tokio::time::Instant::now());
        assert!(!trace.emit(None, EventKind::StepFinished {}, false).await);
        assert!(
            trace
                .emit(
                    None,
                    EventKind::RunFinished {
                        status: RunStatus::Error
                    },
                    true
                )
                .await
        );
        assert!(matches!(
            receiver.recv().await.unwrap().event,
            EventKind::RunFinished { .. }
        ));
    }
}
