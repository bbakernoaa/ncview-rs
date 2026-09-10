use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

use crate::app::Generation;
use crate::data::slice::Slice2D;

#[derive(Debug)]
pub struct WorkerRequest {
    pub generation: Generation,
    pub variable: String,
}

#[derive(Debug)]
pub struct WorkerResult {
    pub generation: Generation,
    pub slice: Result<Slice2D, String>,
}

pub struct WorkerQueue {
    sender: SyncSender<WorkerRequest>,
    receiver: Receiver<WorkerRequest>,
}

impl WorkerQueue {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel(2);
        Self { sender, receiver }
    }

    pub fn submit(&self, request: WorkerRequest) {
        match self.sender.try_send(request) {
            Ok(()) => {}
            Err(TrySendError::Full(request)) => {
                let _ = self.receiver.try_recv();
                let _ = self.sender.try_send(request);
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub fn try_take(&self) -> Option<WorkerRequest> {
        self.receiver.try_iter().last()
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkerQueue, WorkerRequest};
    use crate::app::Generation;

    #[test]
    fn full_queue_drops_oldest_navigation_request() {
        let queue = WorkerQueue::new();
        for generation in 1..=3 { queue.submit(WorkerRequest { generation: Generation(generation), variable: generation.to_string() }); }
        assert_eq!(queue.try_take().unwrap().generation, Generation(3));
    }
}
