use super::{Browse, Client, Root};
use crate::{
    domain::{Destination, DestinationKind, FileCopy, Space},
    hashing::hash_file,
    paths::{join_relative, BACKUP_MARKER_FILE},
    plan::RootResolver,
    transfer::destination_hasher::validate_response,
};
use crate::{
    store::{HashServer, Store},
    sync::normalize_address_with_port,
};

pub fn known_server(store: &Store, id: &str) -> Result<HashServer, String> {
    store
        .hash_servers()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|server| server.id == id)
        .ok_or_else(|| "Hash server is not available on this computer".into())
}

pub async fn add(store: &Store, address: &str, token: &str) -> Result<(), String> {
    if address.trim().starts_with("https://") {
        return Err("Hash servers use plain HTTP; use a trusted LAN or Netbird address".into());
    }
    let address = normalize_address_with_port(address, omb_hash::protocol::DEFAULT_PORT)
        .map_err(|e| e.to_string())?;
    let url = reqwest::Url::parse(&format!("http://{address}"))
        .map_err(|_| "Invalid hash server address")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_none()
        || url.host_str().is_none()
    {
        return Err("Use a host and port, without credentials or URL parameters".into());
    }
    if token.trim().is_empty() || token.contains(['\r', '\n']) {
        return Err("A pairing token is required".into());
    }
    let mut server = HashServer {
        address,
        token: token.trim().into(),
        ..Default::default()
    };
    let hello = Client::new(&server)?.hello().await?;
    server.id = hello.server_id;
    server.name = hello.name;
    server.last_seen = Some(chrono::Utc::now().timestamp_millis());
    store.save_hash_server(&server).map_err(|e| e.to_string())
}

fn record<T>(
    store: &Store,
    mut server: HashServer,
    result: Result<T, String>,
) -> Result<T, String> {
    match &result {
        Ok(_) => {
            server.last_seen = Some(chrono::Utc::now().timestamp_millis());
            server.last_error = None;
        }
        Err(error) => server.last_error = Some(error.clone()),
    }
    store.save_hash_server(&server).map_err(|e| e.to_string())?;
    result
}

pub async fn roots(store: &Store, id: &str) -> Result<Vec<Root>, String> {
    let server = known_server(store, id)?;
    let result = Client::new(&server)?.roots().await;
    record(store, server, result)
}

pub async fn browse(store: &Store, id: &str, root: &str, path: &str) -> Result<Browse, String> {
    let server = known_server(store, id)?;
    let result = Client::new(&server)?.browse(root, path).await;
    record(store, server, result)
}

#[derive(Debug, serde::Serialize)]
pub struct MappingTest {
    pub verified: bool,
    pub message: String,
}

pub fn test_mapping(
    store: &Store,
    resolver: &dyn RootResolver,
    destination_id: &str,
) -> Result<MappingTest, String> {
    let destination: Destination = store
        .get(destination_id)
        .map_err(|e| e.to_string())?
        .ok_or("Destination not found")?;
    if destination.kind != DestinationKind::Folder {
        return Err("Only folder destinations can use remote hashing".into());
    }
    let mapping = destination
        .remote_hash
        .as_ref()
        .ok_or("No remote hash mapping")?;
    let server = known_server(store, &mapping.server_id)?;
    let result = test_mapping_using(store, resolver, &destination, &server);
    record(store, server, result)
}

fn test_mapping_using(
    store: &Store,
    resolver: &dyn RootResolver,
    destination: &Destination,
    server: &HashServer,
) -> Result<MappingTest, String> {
    let mapping = destination.remote_hash.as_ref().expect("checked mapping");
    let root = resolver
        .device_root(&destination.device_id)
        .ok_or("Destination device is not connected")?;
    let canonical = root
        .canonicalize()
        .map_err(|e| format!("Cannot read destination device root: {e}"))?;
    let space: Space = store
        .get(&destination.space_id)
        .map_err(|e| e.to_string())?
        .ok_or("Space not found")?;
    let client = Client::new(server)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(client.browse(&mapping.root, ""))?;
    let copies = store
        .list_by::<FileCopy>("device_id", &destination.device_id)
        .map_err(|e| e.to_string())?;
    let paths = std::iter::once(BACKUP_MARKER_FILE.to_string()).chain(
        copies
            .into_iter()
            .filter(|copy| !copy.removed)
            .map(|copy| copy.path),
    );
    let mut candidate = None;
    for relative in paths {
        omb_hash::protocol::validate_relative(&relative).map_err(|e| e.to_string())?;
        let path = join_relative(&root, &relative);
        match path.canonicalize() {
            Ok(path) if !path.starts_with(&canonical) => {
                return Err("Known file escapes destination device root".into())
            }
            Ok(path) => {
                let metadata = path
                    .metadata()
                    .map_err(|e| format!("Cannot inspect known destination file: {e}"))?;
                if metadata.is_file() {
                    candidate = Some((relative, path));
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Cannot read known destination file: {error}")),
        }
    }
    let Some((relative, path)) = candidate else {
        return Ok(MappingTest {
            verified: false,
            message: "Remote folder exists, but there is no known file or backup marker to compare. Run Test mapping again after copying a file; folder existence alone does not prove the mapping.".into(),
        });
    };
    let before = path.metadata().map_err(|e| e.to_string())?;
    let local = hash_file(&path, space.hash_algo, |_| true).map_err(|e| e.to_string())?;
    let response = runtime.block_on(client.hash(
        &super::HashRequest {
            root: mapping.root.clone(),
            rel_path: relative.clone(),
            algo: space.hash_algo,
        },
        before.len(),
    ))?;
    validate_response(&response, space.hash_algo, before.len())?;
    let after = path.metadata().map_err(|e| e.to_string())?;
    if before.len() != after.len()
        || before.modified().map_err(|e| e.to_string())?
            != after.modified().map_err(|e| e.to_string())?
    {
        return Err("File changed while testing the mapping; try again".into());
    }
    if response.hash != local {
        return Err("Mapping mismatch: local and remote file hashes differ".into());
    }
    Ok(MappingTest {
        verified: true,
        message: format!(
            "Mapping verified: {relative} has the same size and hash locally and on the server."
        ),
    })
}
