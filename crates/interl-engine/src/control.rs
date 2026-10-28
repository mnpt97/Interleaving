//! Shared cancellation signals with priority-ordered stop causes.
use crate::CancellationToken;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};
/// Why execution must stop. Higher discriminants override weaker causes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StopCause {
    /// User cancellation; bounded cleanup remains eligible.
    UserCancel = 1,
    /// The adapter owning the run closed; cleanup must stop.
    OwnerClosed = 2,
    /// The live event consumer disappeared.
    ReceiverClosed = 3,
    /// A resource or safety boundary was reached.
    Safety = 4,
}
/// Cloneable handles to the same cancellation state, safe across Tokio tasks.
/// Cloning shares the Arc and tokens; it does not create an independent run.
#[derive(Clone)]
pub struct RunControl {
    pub(crate) token: CancellationToken,
    pub(crate) shutdown: CancellationToken,
    cause: Arc<AtomicU8>,
}
impl Default for RunControl {
    fn default() -> Self {
        Self::new()
    }
}
impl RunControl {
    /// Creates an uncancelled run control.
    pub fn new() -> Self {
        Self::from_token(CancellationToken::new())
    }
    pub(crate) fn from_token(token: CancellationToken) -> Self {
        Self {
            token,
            shutdown: CancellationToken::new(),
            cause: Arc::new(AtomicU8::new(0)),
        }
    }
    /// Records the strongest cause and signals cancellation.
    /// Causes stronger than user cancellation also interrupt cleanup.
    pub fn stop(&self, cause: StopCause) {
        // Atomic maximum prevents a racing user cancel from weakening owner/safety
        // shutdown. SeqCst gives every task the same ordering of cause updates.
        self.cause.fetch_max(cause as u8, Ordering::SeqCst);
        if cause != StopCause::UserCancel {
            self.shutdown.cancel();
        }
        self.token.cancel();
    }
    /// Returns the strongest observed cause, including direct token cancellation.
    pub fn cause(&self) -> Option<StopCause> {
        match self.cause.load(Ordering::SeqCst) {
            1 => Some(StopCause::UserCancel),
            2 => Some(StopCause::OwnerClosed),
            3 => Some(StopCause::ReceiverClosed),
            4 => Some(StopCause::Safety),
            _ if self.token.is_cancelled() => Some(StopCause::UserCancel),
            _ => None,
        }
    }
    /// Reports whether any shared handle has requested cancellation.
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
    /// Requests user cancellation while allowing eligible cleanup to finish.
    pub fn cancel(&self) {
        self.stop(StopCause::UserCancel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_controls_cannot_weaken_shutdown_with_later_user_cancellation() {
        let control = RunControl::new();
        let owner = control.clone();
        owner.stop(StopCause::OwnerClosed);
        control.cancel();
        assert_eq!(control.cause(), Some(StopCause::OwnerClosed));
        assert!(owner.shutdown.is_cancelled());
    }

    #[test]
    fn user_cancellation_leaves_cleanup_shutdown_token_active() {
        let control = RunControl::new();
        control.cancel();
        assert!(control.is_cancelled());
        assert!(!control.shutdown.is_cancelled());
    }
}
