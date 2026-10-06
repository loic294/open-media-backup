//! Shared test fixture: a store with one space, project, source card, final destination and flow.
use crate::domain::*;
use crate::plan::RootResolver;
use crate::store::Store;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

pub struct HttpServer {
    pub address: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl HttpServer {
    pub fn start(router: axum::Router) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let (send, receive) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, router)
                    .with_graceful_shutdown(async {
                        let _ = receive.await;
                    })
                    .await
                    .unwrap();
            });
        });
        Self {
            address,
            stop: Some(send),
            thread: Some(thread),
        }
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap();
    }
}

pub struct MapResolver(pub parking_lot::Mutex<HashMap<String, PathBuf>>);

impl RootResolver for MapResolver {
    fn device_root(&self, device_id: &str) -> Option<PathBuf> {
        self.0.lock().get(device_id).cloned()
    }
}

pub struct Fixture {
    pub store: Arc<Store>,
    pub resolver: MapResolver,
    pub card_dir: TempDir,
    pub nas_dir: TempDir,
    pub space: Space,
    pub project: Project,
    pub card: Device,
    pub nas: Device,
    pub source: Source,
    pub destination: Destination,
    pub flow: Flow,
}

impl Fixture {
    pub fn new() -> Self {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let (card_dir, nas_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let space = Space {
            id: "space".into(),
            name: "Travel".into(),
            final_copies_required: 1,
            ..Default::default()
        };
        let project = Project {
            id: "project".into(),
            space_id: space.id.clone(),
            name: "Trip".into(),
            ..Default::default()
        };
        let card = Device {
            id: "card".into(),
            name: "Camera A Card 1".into(),
            role: DeviceRole::Original,
            ..Default::default()
        };
        let nas = Device {
            id: "nas".into(),
            name: "Home NAS".into(),
            role: DeviceRole::Final,
            ..Default::default()
        };
        let source = Source {
            id: "src".into(),
            space_id: space.id.clone(),
            device_id: card.id.clone(),
            path_template: "DCIM".into(),
            offer_wipe: true,
            position: 0,
            ..Default::default()
        };
        let destination = Destination {
            id: "dst".into(),
            space_id: space.id.clone(),
            device_id: nas.id.clone(),
            path_template: "photo/Trip".into(),
            ..Default::default()
        };
        let flow = Flow {
            id: "flow".into(),
            space_id: space.id.clone(),
            source_id: source.id.clone(),
            destination_id: destination.id.clone(),
        };
        store.put(&space).unwrap();
        store.put(&project).unwrap();
        store.put_all(&[card.clone(), nas.clone()]).unwrap();
        store.put(&source).unwrap();
        store.put(&destination).unwrap();
        store.put(&flow).unwrap();
        let roots = HashMap::from([
            (card.id.clone(), card_dir.path().to_path_buf()),
            (nas.id.clone(), nas_dir.path().to_path_buf()),
        ]);
        Self {
            store,
            resolver: MapResolver(parking_lot::Mutex::new(roots)),
            card_dir,
            nas_dir,
            space,
            project,
            card,
            nas,
            source,
            destination,
            flow,
        }
    }

    pub fn write_card_file(&self, rel: &str, content: &[u8]) -> PathBuf {
        write(self.card_dir.path(), rel, content)
    }

    pub fn unmount(&self, device_id: &str) {
        self.resolver.0.lock().remove(device_id);
    }
}

pub fn write(root: &Path, rel: &str, content: &[u8]) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    path
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}
