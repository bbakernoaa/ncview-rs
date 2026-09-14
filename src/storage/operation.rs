//! Background remote operation state and progress messages.

use std::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{app::Generation, storage::location::SourceLocation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Open,
    Metadata,
    Index,
    Slice,
    Coordinates,
    TimeSeries,
    Render,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationPhase {
    Queued,
    Head,
    Discovering,
    Fetching,
    Decoding,
    Rendering,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub message: String,
    pub completed_units: Option<u64>,
    pub total_units: Option<u64>,
    pub requested_bytes: u64,
    pub received_bytes: u64,
    pub retries: u32,
    pub cancellable: bool,
}

#[derive(Debug, Clone)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RemoteOperation {
    pub id: u64,
    pub generation: Generation,
    pub kind: OperationKind,
    pub source: SourceLocation,
    pub phase: OperationPhase,
    pub progress: Progress,
    pub cancellation: CancellationToken,
}

impl RemoteOperation {
    pub fn new(
        id: u64,
        generation: Generation,
        kind: OperationKind,
        source: SourceLocation,
    ) -> Self {
        Self {
            id,
            generation,
            kind,
            source,
            phase: OperationPhase::Queued,
            progress: Progress::default(),
            cancellation: CancellationToken::new(),
        }
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn advance(&mut self, phase: OperationPhase, message: impl Into<String>) {
        self.phase = phase;
        self.progress.message = message.into();
    }
}

#[derive(Debug, Default)]
pub struct LatestGeneration {
    current: AtomicU64,
}

impl LatestGeneration {
    pub fn accept(&self, generation: Generation) -> bool {
        let mut current = self.current.load(Ordering::Acquire);
        loop {
            if generation.0 < current {
                return false;
            }
            match self.current.compare_exchange(
                current,
                generation.0,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(observed) => current = observed,
            }
        }
    }

    pub fn current(&self) -> Generation {
        Generation(self.current.load(Ordering::Acquire))
    }
}

pub fn retry_delay(attempt: u32, base: Duration, maximum: Duration) -> Duration {
    let multiplier = 1_u32.checked_shl(attempt.min(16)).unwrap_or(u32::MAX);
    base.saturating_mul(multiplier).min(maximum)
}

pub fn is_retryable_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "timeout",
        "timed out",
        "connection reset",
        "connection refused",
        "temporarily unavailable",
        "too many requests",
        "service unavailable",
        "internal server error",
        "bad gateway",
        "gateway timeout",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[derive(Debug, Default)]
pub struct OperationState {
    active: Mutex<Option<RemoteOperation>>,
    next_id: AtomicU64,
}

impl OperationState {
    pub fn begin(
        &self,
        generation: Generation,
        kind: OperationKind,
        source: SourceLocation,
    ) -> RemoteOperation {
        let operation = RemoteOperation::new(
            self.next_id.fetch_add(1, Ordering::Relaxed),
            generation,
            kind,
            source,
        );
        if let Ok(mut active) = self.active.lock() {
            *active = Some(operation.clone());
        }
        operation
    }

    pub fn active(&self) -> Option<RemoteOperation> {
        self.active.lock().ok()?.clone()
    }

    pub fn update(&self, id: u64, phase: OperationPhase, progress: Progress) -> bool {
        let Ok(mut active) = self.active.lock() else {
            return false;
        };
        let Some(operation) = active.as_mut() else {
            return false;
        };
        if operation.id != id {
            return false;
        }
        operation.phase = phase;
        operation.progress = progress;
        true
    }

    pub fn cancel(&self, id: u64) -> bool {
        let Some(operation) = self.active() else {
            return false;
        };
        if operation.id != id {
            return false;
        }
        operation.cancel();
        if let Ok(mut active) = self.active.lock()
            && let Some(operation) = active.as_mut()
            && operation.id == id
        {
            operation.phase = OperationPhase::Cancelled;
        }
        true
    }

    pub fn finish(&self, id: u64, phase: OperationPhase) -> bool {
        let Ok(mut active) = self.active.lock() else {
            return false;
        };
        let Some(operation) = active.as_mut() else {
            return false;
        };
        if operation.id != id {
            return false;
        }
        operation.phase = phase;
        true
    }
}
