use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub struct Config {
    pub id: String,
    pub name: String,
    pub token: String,
    pub port: u16,
    pub roots: Vec<PathBuf>,
}

impl Config {
    pub fn from_env() -> io::Result<Self> {
        let config =
            PathBuf::from(env::var("OMB_HASH_CONFIG").unwrap_or_else(|_| "/config".into()));
        fs::create_dir_all(&config)?;
        let id = persistent_value(&config.join("id"), || Uuid::new_v4().to_string())?;
        Uuid::parse_str(&id).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let token = match env::var("OMB_HASH_TOKEN") {
            Ok(token) => token,
            Err(env::VarError::NotPresent) => persistent_value(&config.join("token"), || {
                format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
            })?,
            Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidInput, e)),
        };
        if token.is_empty() || token.contains(['\r', '\n']) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid token"));
        }
        let port = env::var("OMB_HASH_PORT")
            .unwrap_or_else(|_| crate::DEFAULT_PORT.to_string())
            .parse::<u16>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let roots = match env::var("OMB_HASH_ROOTS") {
            Ok(value) => serde_json::from_str::<Vec<PathBuf>>(&value)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
            Err(env::VarError::NotPresent) => discover_roots(Path::new("/data"))?,
            Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidInput, e)),
        };
        if roots.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no exposed roots",
            ));
        }
        Ok(Self {
            id,
            token,
            port,
            roots,
            name: env::var("OMB_HASH_NAME").unwrap_or_else(|_| "NAS hash server".into()),
        })
    }
}

pub fn discover_roots(data: &Path) -> io::Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    for entry in fs::read_dir(data)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            roots.push(entry.path());
        }
    }
    roots.sort();
    Ok(roots)
}

fn persistent_value(path: &Path, generate: impl FnOnce() -> String) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(value) if !value.trim().is_empty() => return Ok(value.trim().into()),
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "empty persisted configuration",
            ))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let value = generate();
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(value.as_bytes())?;
    file.sync_all()?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roots_are_only_top_level_directories_and_config_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("share")).unwrap();
        fs::write(dir.path().join("file"), b"no").unwrap();
        assert_eq!(
            discover_roots(dir.path()).unwrap(),
            vec![dir.path().join("share")]
        );
        let path = dir.path().join("id");
        let first = persistent_value(&path, || "first".into()).unwrap();
        assert_eq!(persistent_value(&path, || "second".into()).unwrap(), first);
        fs::write(path, "").unwrap();
        assert!(persistent_value(&dir.path().join("id"), || "third".into()).is_err());
    }
}
