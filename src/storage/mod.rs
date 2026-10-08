//! Provider-neutral remote object access.
//!
//! Network work belongs behind this module and its background runtime. The UI and format adapters
//! exchange validated ranges and typed operation messages rather than provider-specific requests.

pub mod location;
pub mod object_store;
pub mod operation;
pub mod range_cache;
pub mod session;

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
        mpsc::SyncSender,
    },
    thread,
    time::Duration,
};

use crate::error::{NcvError, Result};

type Job = Box<dyn FnOnce(&tokio::runtime::Runtime) + Send + 'static>;

/// A bounded bridge to the one Tokio runtime used for remote object operations.
///
/// The caller only enqueues a future and receives a standard-library receiver. Runtime creation,
/// polling, and all network work happen on the dedicated worker thread.
#[derive(Clone)]
pub struct StorageRuntime {
    jobs: SyncSender<Job>,
}

impl StorageRuntime {
    pub fn spawn() -> Result<Self> {
        let (jobs, incoming) = mpsc::sync_channel::<Job>(32);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("ncv-storage-runtime".to_owned())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_sender.send(Err(error.to_string()));
                        return;
                    }
                };
                let _ = ready_sender.send(Ok(()));
                while let Ok(job) = incoming.recv() {
                    job(&runtime);
                }
            })
            .map_err(|error| NcvError::RemoteOperation {
                location: Box::new(location_for_runtime_error()),
                operation: "runtime".to_owned(),
                reason: error.to_string(),
            })?;
        ready_receiver
            .recv()
            .map_err(|_| NcvError::WorkerStopped)?
            .map_err(|reason| NcvError::RemoteOperation {
                location: Box::new(location_for_runtime_error()),
                operation: "runtime".to_owned(),
                reason,
            })?;
        Ok(Self { jobs })
    }

    pub fn submit<F, T>(&self, future: F) -> Result<mpsc::Receiver<T>>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.jobs
            .try_send(Box::new(move |runtime| {
                let value = runtime.block_on(future);
                let _ = sender.send(value);
            }))
            .map_err(|_| NcvError::WorkerStopped)?;
        Ok(receiver)
    }

    /// Submit a provider operation that can be cancelled by dropping its
    /// future. This is intentionally separate from `submit`: existing callers
    /// that do not own an operation token keep the simple result channel.
    pub fn submit_cancellable<F, T>(
        &self,
        future: F,
        cancelled: Arc<AtomicBool>,
    ) -> Result<mpsc::Receiver<Option<T>>>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.jobs
            .try_send(Box::new(move |runtime| {
                let value = runtime.block_on(async move {
                    tokio::select! {
                        value = future => Some(value),
                        _ = wait_for_cancellation(cancelled) => None,
                    }
                });
                let _ = sender.send(value);
            }))
            .map_err(|_| NcvError::WorkerStopped)?;
        Ok(receiver)
    }
}

pub(crate) fn receive<F, T>(runtime: &StorageRuntime, future: F) -> Result<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    runtime
        .submit(future)?
        .recv()
        .map_err(|_| NcvError::WorkerStopped)
}

async fn wait_for_cancellation(cancelled: Arc<AtomicBool>) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn location_for_runtime_error() -> location::SourceLocation {
    location::SourceLocation::parse("<storage-runtime>").expect("runtime location is valid")
}
