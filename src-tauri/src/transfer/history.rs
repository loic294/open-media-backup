use super::AnalysisJob;
use crate::store::Store;
use parking_lot::Mutex;
use std::sync::{mpsc, Arc};

struct Record {
    job: AnalysisJob,
    ack: Option<mpsc::Sender<()>>,
}

/// One ordered writer keeps SQLite off the job/manager locks and chunk I/O path.
#[derive(Clone)]
pub(crate) struct HistoryRecorder {
    sender: mpsc::Sender<Record>,
    error: Arc<Mutex<Option<String>>>,
}

impl HistoryRecorder {
    pub fn new(store: Arc<Store>) -> Self {
        let (sender, receiver) = mpsc::channel::<Record>();
        let error = Arc::new(Mutex::new(None));
        let writer_error = error.clone();
        std::thread::spawn(move || {
            for record in receiver {
                if let Err(e) = store.save_analysis_job(&record.job) {
                    let message = format!(
                        "Could not save speed analysis for job {}: {e}",
                        record.job.id
                    );
                    log::error!("{message}");
                    *writer_error.lock() = Some(message);
                }
                if let Some(ack) = record.ack {
                    let _ = ack.send(());
                }
            }
        });
        Self { sender, error }
    }

    pub fn record(&self, job: AnalysisJob, wait: bool) -> Option<mpsc::Receiver<()>> {
        let (ack, receiver) = if wait {
            let (sender, receiver) = mpsc::channel();
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        if let Err(e) = self.sender.send(Record { job, ack }) {
            let message = format!("Speed analysis writer stopped: {e}");
            log::error!("{message}");
            *self.error.lock() = Some(message);
        }
        receiver
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().clone()
    }
}
