use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::core::cancel::CancelToken;
use crate::core::error::AppError;

struct LeaseState {
    token: CancelToken,
    active: AtomicBool,
}

pub struct OperationGate {
    current: Arc<Mutex<Option<Arc<LeaseState>>>>,
}

impl Default for OperationGate {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationGate {
    pub fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(None)),
        }
    }

    pub fn create(&self) -> Result<OperationLease, AppError> {
        let mut guard = self
            .current
            .lock()
            .map_err(|_| AppError::general("Operation gate is unavailable."))?;
        if let Some(existing) = guard.as_ref() {
            if existing.active.load(Ordering::SeqCst) && !existing.token.is_cancelled() {
                return Err(AppError::OperationConflict);
            }
        }
        if let Some(existing) = guard.take() {
            existing.token.cancel();
        }
        let state = Arc::new(LeaseState {
            token: CancelToken::new(),
            active: AtomicBool::new(true),
        });
        *guard = Some(state.clone());
        Ok(OperationLease {
            state,
            gate: Arc::downgrade(&self.current),
        })
    }

    pub fn cancel_current(&self) {
        if let Ok(guard) = self.current.lock() {
            if let Some(state) = guard.as_ref() {
                state.token.cancel();
            }
        }
    }
}

pub struct OperationLease {
    state: Arc<LeaseState>,
    gate: Weak<Mutex<Option<Arc<LeaseState>>>>,
}

impl OperationLease {
    pub fn token(&self) -> CancelToken {
        self.state.token.clone()
    }

    pub fn is_cancellation_requested(&self) -> bool {
        self.state.token.is_cancelled()
    }

    pub fn cancel(&self) {
        if self.state.active.load(Ordering::SeqCst) {
            self.state.token.cancel();
        }
    }
}

impl Drop for OperationLease {
    fn drop(&mut self) {
        if !self.state.active.swap(false, Ordering::SeqCst) {
            return;
        }
        if let Some(gate) = self.gate.upgrade() {
            if let Ok(mut guard) = gate.lock() {
                let matches = guard
                    .as_ref()
                    .map(|current| Arc::ptr_eq(current, &self.state))
                    .unwrap_or(false);
                if matches {
                    *guard = None;
                }
            }
        }
    }
}
