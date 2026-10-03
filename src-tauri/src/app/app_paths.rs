use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum AppPathOs {
    Macos,
    Windows,
    Linux,
}

impl AppPathOs {
    #[cfg(target_os = "macos")]
    fn current() -> Self {
        Self::Macos
    }

    #[cfg(target_os = "windows")]
    fn current() -> Self {
        Self::Windows
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn current() -> Self {
        Self::Linux
    }
}

pub fn validate_app_path(path: &Path) -> Result<PathBuf, String> {
    validate_app_path_for_os(path, AppPathOs::current())
}

fn validate_app_path_for_os(path: &Path, os: AppPathOs) -> Result<PathBuf, String> {
    let resolved = path
        .canonicalize()
        .map_err(|error| format!("Could not resolve application path: {error}"))?;
    let normalized = match os {
        AppPathOs::Macos => macos_app_bundle_root(&resolved).unwrap_or(resolved),
        AppPathOs::Windows | AppPathOs::Linux => resolved,
    };
    match os {
        AppPathOs::Macos => validate_macos_app(&normalized)?,
        AppPathOs::Windows => validate_windows_app(&normalized)?,
        AppPathOs::Linux => validate_linux_app(&normalized)?,
    }
    Ok(normalized)
}

pub fn normalize_app_settings(
    next: &mut super::settings::AppSettings,
    current: &super::settings::AppSettings,
) -> Result<(), String> {
    normalize_optional_path(&mut next.preview_apps.photos, &current.preview_apps.photos)?;
    normalize_optional_path(&mut next.preview_apps.videos, &current.preview_apps.videos)?;
    for (destination_id, app_path) in &mut next.app_destinations {
        if current
            .app_destinations
            .get(destination_id)
            .is_some_and(|current_path| current_path == app_path)
        {
            continue;
        }
        *app_path = path_to_string(&validate_app_path(Path::new(app_path))?);
    }
    Ok(())
}

fn normalize_optional_path(
    next: &mut Option<String>,
    current: &Option<String>,
) -> Result<(), String> {
    let Some(path) = next else {
        return Ok(());
    };
    if current
        .as_ref()
        .is_some_and(|current_path| current_path == path)
    {
        return Ok(());
    }
    *path = path_to_string(&validate_app_path(Path::new(path))?);
    Ok(())
}

pub fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn macos_app_bundle_root(path: &Path) -> Option<PathBuf> {
    let mut root = PathBuf::new();
    for component in path.components() {
        root.push(component.as_os_str());
        if component
            .as_os_str()
            .to_string_lossy()
            .to_lowercase()
            .ends_with(".app")
        {
            return Some(root);
        }
    }
    None
}

fn validate_macos_app(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("Application does not exist".into());
    }
    if !path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().to_lowercase().ends_with(".app"))
    {
        return Err("Choose a macOS .app application".into());
    }
    if !path.is_dir() {
        return Err("macOS applications must be .app bundles".into());
    }
    Ok(())
}

fn validate_windows_app(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("Choose an application file".into());
    }
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase());
    match extension.as_deref() {
        Some("exe") | Some("lnk") => Ok(()),
        _ => Err("Choose a Windows .exe or .lnk application".into()),
    }
}

fn validate_linux_app(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("Choose an executable or .desktop file".into());
    }
    if path
        .extension()
        .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("desktop"))
    {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path
            .metadata()
            .map_err(|error| format!("Could not inspect application path: {error}"))?
            .permissions()
            .mode()
            & 0o111
            != 0
        {
            return Ok(());
        }
    }
    Err("Choose an executable or .desktop file".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::current_dir()
            .expect("cwd")
            .join("target")
            .join("app-path-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("test dir");
        dir
    }

    #[test]
    fn macos_inside_bundle_path_normalizes_to_app_root() {
        let dir = test_dir("inside-bundle");
        let app = dir.join("Adobe Lightroom Classic.app");
        let executable_dir = app.join("Contents").join("MacOS");
        fs::create_dir_all(&executable_dir).unwrap();
        let executable = executable_dir.join("Lightroom");
        fs::write(&executable, "").unwrap();

        let normalized = validate_app_path_for_os(&executable, AppPathOs::Macos).unwrap();

        assert_eq!(normalized, app.canonicalize().unwrap());
    }

    #[test]
    fn macos_rejects_existing_non_app_path() {
        let dir = test_dir("non-app");
        let path = dir.join("Lightroom");
        fs::write(&path, "").unwrap();

        let error = validate_app_path_for_os(&path, AppPathOs::Macos).unwrap_err();

        assert!(error.contains(".app"));
    }

    #[test]
    fn macos_rejects_missing_app_bundle() {
        let dir = test_dir("missing");
        let error =
            validate_app_path_for_os(&dir.join("Missing.app"), AppPathOs::Macos).unwrap_err();

        assert!(error.contains("Could not resolve"));
    }

    #[test]
    fn macos_accepts_app_bundle_root() {
        let dir = test_dir("app-root");
        let app = dir.join("Capture One.app");
        fs::create_dir_all(&app).unwrap();

        let normalized = validate_app_path_for_os(&app, AppPathOs::Macos).unwrap();

        assert_eq!(normalized, app.canonicalize().unwrap());
    }
}
