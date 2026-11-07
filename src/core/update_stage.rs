//! Stage cleanup must never consume the last copy of an installation.

use std::io;
use std::path::Path;

pub(crate) const RECOVERY_BACKUPS: [&str; 2] = ["previous.app", "previous.AppImage"];
pub(crate) const RUNNING_PROCESS: &str = ".tty7-update-process";

pub(crate) fn mark_running_process(stage: &Path, pid: u32, detail: &str) -> io::Result<()> {
    std::fs::write(
        stage.join(RUNNING_PROCESS),
        format!("process: {pid}\n{detail}\nstage: {}\n", stage.display()),
    )
}

pub(crate) fn needs_recovery(stage: &Path) -> bool {
    RECOVERY_BACKUPS
        .into_iter()
        .chain([RUNNING_PROCESS])
        .any(|name| {
            // An unreadable entry is not evidence that the backup is absent.
            match std::fs::symlink_metadata(stage.join(name)) {
                Ok(_) => true,
                Err(error) => error.kind() != io::ErrorKind::NotFound,
            }
        })
}

pub(crate) fn remove_stage(stage: &Path) -> io::Result<()> {
    if needs_recovery(stage) {
        return Err(io::Error::other(format!(
            "update recovery files are preserved at {}",
            stage.display()
        )));
    }
    match std::fs::symlink_metadata(stage) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            std::fs::remove_dir_all(stage)
        }
        Ok(_) => Err(io::Error::other("the update stage is not a directory")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_keeps_each_recovery_backup() {
        for name in RECOVERY_BACKUPS.into_iter().chain([RUNNING_PROCESS]) {
            let root = tempfile::tempdir().unwrap();
            let stage = root.path().join("stage");
            std::fs::create_dir(&stage).unwrap();
            std::fs::write(stage.join(name), b"keep").unwrap();
            assert!(remove_stage(&stage).is_err());
            assert_eq!(std::fs::read(stage.join(name)).unwrap(), b"keep");
        }
    }

    #[test]
    fn ordinary_and_already_removed_stages_can_be_cleaned() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join("stage");
        std::fs::create_dir(&stage).unwrap();
        std::fs::write(stage.join("download.zip"), b"package").unwrap();
        remove_stage(&stage).unwrap();
        remove_stage(&stage).unwrap();
        assert!(!stage.exists());
    }
}
