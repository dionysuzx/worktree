use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;

pub(crate) struct RepoLock {
    _file: File,
}

impl RepoLock {
    pub(crate) fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("failed to lock {}", path.display()))?;
        Ok(Self { _file: file })
    }
}
