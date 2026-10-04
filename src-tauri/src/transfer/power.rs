use parking_lot::Mutex;
use std::any::Any;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

type Guard = Box<dyn Any>;
type Factory = Box<dyn FnMut() -> Result<Guard, String> + Send>;
type Warning = Arc<dyn Fn(String) + Send + Sync>;

enum Change {
    Enabled(bool),
    Active(String, bool),
    Shutdown,
}

struct Request {
    change: Change,
    done: mpsc::SyncSender<()>,
}

struct Inner {
    sender: mpsc::Sender<Request>,
    worker: Mutex<Option<JoinHandle<()>>>,
    warning: Warning,
    worker_failure_reported: AtomicBool,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.get_mut().take() {
            let (done, _) = mpsc::sync_channel(1);
            if self
                .sender
                .send(Request {
                    change: Change::Shutdown,
                    done,
                })
                .is_err()
            {
                log::error!("Sleep prevention controller already stopped");
            }
            if worker.join().is_err() {
                log::error!("Sleep prevention controller panicked");
            }
        }
    }
}

/// The native guard never leaves its owning thread: Windows power requests are thread-bound.
#[derive(Clone)]
pub(crate) struct PowerController {
    inner: Arc<Inner>,
}

impl PowerController {
    pub(crate) fn new(enabled: bool, warning: impl Fn(String) + Send + Sync + 'static) -> Self {
        Self::with_factory(
            enabled,
            Box::new(|| {
                // Ordinary unit/IPC tests must not change the host's sleep settings.
                if cfg!(test) {
                    Ok(Box::new(()))
                } else {
                    native_guard()
                }
            }),
            Arc::new(warning),
        )
    }

    pub(super) fn with_factory(enabled: bool, factory: Factory, warning: Warning) -> Self {
        let (sender, receiver) = mpsc::channel();
        let on_warning = warning.clone();
        let worker = std::thread::Builder::new()
            .name("transfer-power".into())
            .spawn(move || run(receiver, enabled, factory, on_warning))
            .map_err(|error| log::error!("Could not start sleep prevention controller: {error}"))
            .ok();
        Self {
            inner: Arc::new(Inner {
                sender,
                worker: Mutex::new(worker),
                warning,
                worker_failure_reported: AtomicBool::new(false),
            }),
        }
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.change(Change::Enabled(enabled));
    }

    pub(super) fn set_active(&self, id: &str, active: bool) {
        self.change(Change::Active(id.to_string(), active));
    }

    fn change(&self, change: Change) {
        let (done, acknowledged) = mpsc::sync_channel(1);
        let request = Request { change, done };
        if self.inner.sender.send(request).is_err() || acknowledged.recv().is_err() {
            log::error!("Sleep prevention controller is unavailable");
            if !self
                .inner
                .worker_failure_reported
                .swap(true, Ordering::SeqCst)
            {
                (self.inner.warning)(warning_message("the power controller stopped"));
            }
        }
    }
}

pub(super) fn native_guard() -> Result<Guard, String> {
    keepawake::Builder::default()
        .display(false)
        .idle(true)
        .sleep(false)
        .reason("Open Media Backup transfer")
        .app_name("Open Media Backup")
        .app_reverse_domain("me.loicba.open-media-backup")
        .create()
        .map(|guard| Box::new(guard) as Guard)
        .map_err(|error| error.to_string())
}

fn warning_message(error: &str) -> String {
    format!(
        "Could not keep this computer awake: {error}. Transfers will continue, \
         but automatic sleep may interrupt them."
    )
}

fn run(
    receiver: mpsc::Receiver<Request>,
    mut enabled: bool,
    mut factory: Factory,
    warning: Warning,
) {
    let mut active = HashSet::new();
    let mut guard: Option<Guard> = None;
    let mut warned = false;
    while let Ok(request) = receiver.recv() {
        let activate = match request.change {
            Change::Enabled(value) => {
                let changed = enabled != value;
                enabled = value;
                changed && value
            }
            Change::Active(id, true) => active.insert(id),
            Change::Active(id, false) => {
                active.remove(&id);
                false
            }
            Change::Shutdown => break,
        };
        if !enabled || active.is_empty() {
            guard = None;
            warned = false;
        } else if guard.is_none() && activate {
            match factory() {
                Ok(acquired) => guard = Some(acquired),
                Err(error) => {
                    log::warn!("Could not prevent automatic sleep during transfers: {error}");
                    if !warned {
                        warning(warning_message(&error));
                        warned = true;
                    }
                }
            }
        }
        // Work may start/resume only once acquisition (or the visible warning) has been handled.
        if request.done.send(()).is_err() {
            log::error!("Sleep prevention request caller disconnected");
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::thread::{self, ThreadId};

    #[derive(Default)]
    pub(crate) struct Probe {
        pub acquired: AtomicUsize,
        pub released: AtomicUsize,
        pub warnings: Mutex<Vec<String>>,
    }

    struct TestGuard {
        probe: Arc<Probe>,
        owner: ThreadId,
    }

    impl Drop for TestGuard {
        fn drop(&mut self) {
            assert_eq!(self.owner, thread::current().id());
            self.probe.released.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub(crate) fn controller(enabled: bool) -> (PowerController, Arc<Probe>) {
        let probe = Arc::new(Probe::default());
        let factory_probe = probe.clone();
        let warning_probe = probe.clone();
        let controller = PowerController::with_factory(
            enabled,
            Box::new(move || {
                factory_probe.acquired.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(TestGuard {
                    probe: factory_probe.clone(),
                    owner: thread::current().id(),
                }))
            }),
            Arc::new(move |warning| warning_probe.warnings.lock().push(warning)),
        );
        (controller, probe)
    }

    #[test]
    fn shares_guard_and_releases_on_last_job_and_shutdown_on_owner_thread() {
        let (controller, probe) = controller(true);
        controller.set_active("a", false);
        assert_eq!(probe.acquired.load(Ordering::SeqCst), 0);
        controller.set_active("a", true);
        controller.set_active("a", true);
        controller.set_active("b", true);
        assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
        controller.set_active("a", false);
        assert_eq!(probe.released.load(Ordering::SeqCst), 0);
        controller.set_active("b", false);
        assert_eq!(probe.released.load(Ordering::SeqCst), 1);
        controller.set_active("a", true);
        assert_eq!(probe.acquired.load(Ordering::SeqCst), 2);
        drop(controller);
        assert_eq!(probe.released.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn enabling_and_disabling_reconciles_existing_active_jobs() {
        let (controller, probe) = controller(false);
        controller.set_active("a", true);
        assert_eq!(probe.acquired.load(Ordering::SeqCst), 0);
        controller.set_enabled(true);
        assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
        controller.set_enabled(true);
        assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
        controller.set_enabled(false);
        assert_eq!(probe.released.load(Ordering::SeqCst), 1);
        controller.set_enabled(true);
        assert_eq!(probe.acquired.load(Ordering::SeqCst), 2);
        controller.set_active("a", false);
        assert_eq!(probe.released.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn failure_is_non_fatal_deduplicated_and_retried_on_new_activation() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = attempts.clone();
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let sink = warnings.clone();
        let controller = PowerController::with_factory(
            true,
            Box::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
                Err("OS refused".into())
            }),
            Arc::new(move |warning| sink.lock().push(warning)),
        );
        controller.set_active("a", true);
        controller.set_active("a", true);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(warnings.lock().len(), 1);
        controller.set_active("b", true);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(warnings.lock().len(), 1);
        controller.set_enabled(false);
        controller.set_enabled(true);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(warnings.lock().len(), 2);
        controller.set_active("a", false);
        controller.set_active("b", false);
        controller.set_active("a", true);
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
        assert_eq!(warnings.lock().len(), 3);
        assert!(warnings.lock()[0].contains("Transfers will continue"));
    }
}
