use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

pub(crate) const CONFIG_FILE_NAME: &str = "connections.toml";

static WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);
static CONFIG_FILE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn lock() -> MutexGuard<'static, ()> {
    CONFIG_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn get_config_dir() -> Result<PathBuf, std::io::Error> {
    let config_base = dirs::config_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find config directory",
        )
    })?;
    Ok(config_base.join("sabiql"))
}

pub(crate) fn config_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CONFIG_FILE_NAME)
}

pub(crate) fn write_config_file(config_dir: &Path, content: &str) -> Result<(), std::io::Error> {
    if !config_dir.exists() {
        fs::create_dir_all(config_dir)?;
    }

    let path = config_file_path(config_dir);
    let counter = WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = config_dir.join(format!(
        ".connections.toml.{}-{}.tmp",
        std::process::id(),
        counter,
    ));

    if let Err(e) = fs::write(&tmp_path, content) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    if let Err(e) = set_file_permissions(&tmp_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    if let Err(e) = fs::rename(&tmp_path, &path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    Ok(())
}

pub(crate) fn render_config_file(content: &str) -> String {
    format!(
        "# sabiql connection configuration\n# WARNING: Passwords are stored in plain text\n\n{content}"
    )
}

#[cfg(unix)]
fn set_file_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}
