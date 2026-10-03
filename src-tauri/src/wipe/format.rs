use std::path::Path;
use std::process::Command;

/// Volume labels are limited (exFAT: 11 chars on macOS diskutil, 15 on Windows).
fn label(name: &str) -> String {
    let clean: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '_').collect();
    let trimmed: String = clean.trim().chars().take(11).collect();
    if trimmed.is_empty() { "MEDIA".into() } else { trimmed.to_uppercase() }
}

fn run(mut cmd: Command) -> Result<(), String> {
    let output = cmd.output().map_err(|e| format!("could not start formatter: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("format failed: {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

/// Erases the volume mounted at `root` as exFAT, keeping it mounted at the same place when possible.
pub fn quick_format(root: &Path, name: &str) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        if !root.starts_with("/Volumes/") || root.components().count() != 3 {
            return Err("refusing to format something that is not an external volume".into());
        }
        let mut cmd = Command::new("diskutil");
        cmd.args(["eraseVolume", "ExFAT", &label(name)]).arg(root);
        run(cmd)
    } else if cfg!(target_os = "windows") {
        let letter = root.to_string_lossy().chars().next().filter(|c| c.is_ascii_alphabetic()).ok_or("no drive letter")?;
        if letter.eq_ignore_ascii_case(&'C') {
            return Err("refusing to format the system drive".into());
        }
        let script = format!(
            "Format-Volume -DriveLetter {letter} -FileSystem exFAT -NewFileSystemLabel '{}' -Force -Confirm:$false",
            label(name)
        );
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        run(cmd)
    } else {
        Err("quick format is not supported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn labels_are_safe() {
        assert_eq!(super::label("A7IV Card #1 extra"), "A7IV CARD 1");
        assert_eq!(super::label("ü/"), "MEDIA");
    }
}
