//! GPUI-free core: config, rule engine, browser discovery and launching.

pub mod browsers;
pub mod config;
pub mod history;
#[cfg(windows)]
pub mod icons;
pub mod launch;
pub mod registration;
pub mod rules;
pub mod source;

use std::path::{Path, PathBuf};

/// Writes via a temp file + rename so readers never see a half-written file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// `%APPDATA%\Crossroads`: roaming settings (rules).
pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("Crossroads")
}

/// `%LOCALAPPDATA%\Crossroads`: machine-local caches (icons, history).
pub fn local_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("Crossroads")
}

#[cfg(test)]
pub(crate) mod tempdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A unique temp directory removed on drop.
    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "crossroads-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
