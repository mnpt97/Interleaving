use crate::CancellationToken;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StopCause {
    UserCancel = 1,
    OwnerClosed = 2,
    ReceiverClosed = 3,
    Safety = 4,
}
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
    pub fn stop(&self, cause: StopCause) {
        self.cause.fetch_max(cause as u8, Ordering::SeqCst);
        if cause != StopCause::UserCancel {
            self.shutdown.cancel();
        }
        self.token.cancel();
    }
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
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
    pub fn cancel(&self) {
        self.stop(StopCause::UserCancel);
    }
}
