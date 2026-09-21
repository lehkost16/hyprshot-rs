//! Shared recorder/overlay lifecycle for video recording and scrolling capture.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};

mod process;
use process::ProcessIdentity;

use crate::compositor::MonitorInfo;
use crate::geometry::Geometry;

pub struct StartedCapture {
    recorder: Child,
    overlay: Option<Child>,
    committed: bool,
}

impl StartedCapture {
    pub fn start(
        mut recorder: Command,
        geometry: &Geometry,
        monitor: &MonitorInfo,
        debug: bool,
    ) -> Result<Self> {
        let mut capture = Self {
            recorder: recorder
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("Failed to start wl-screenrec")?,
            overlay: None,
            committed: false,
        };
        let stderr = if debug {
            Stdio::from(std::fs::File::create(crate::utils::runtime_state_path(
                "overlay.log",
            ))?)
        } else {
            Stdio::null()
        };
        capture.overlay = Some(
            Command::new(std::env::current_exe()?)
                .arg("overlay")
                .arg("--x")
                .arg(geometry.x.to_string())
                .arg("--y")
                .arg(geometry.y.to_string())
                .arg("--w")
                .arg(geometry.width.to_string())
                .arg("--h")
                .arg(geometry.height.to_string())
                .arg("--scale")
                .arg(monitor.scale.to_string())
                .arg("--monitor")
                .arg(&monitor.name)
                .arg("--ox")
                .arg(monitor.x.to_string())
                .arg("--oy")
                .arg(monitor.y.to_string())
                .args(debug.then_some("--debug"))
                .stdout(Stdio::null())
                .stderr(stderr)
                .spawn()
                .context("Failed to start capture overlay")?,
        );
        Ok(capture)
    }

    pub fn session(&self) -> Result<Session> {
        Ok(Session {
            phase: Phase::Recording,
            recorder: ProcessIdentity::capture(self.recorder.id())?,
            overlay: ProcessIdentity::capture(
                self.overlay.as_ref().context("Overlay not started")?.id(),
            )?,
        })
    }

    pub fn commit(mut self, path: &Path, state: &impl Serialize) -> Result<()> {
        ensure!(
            self.recorder.try_wait()?.is_none(),
            "Recorder exited before session was ready"
        );
        if let Some(overlay) = self.overlay.as_mut() {
            ensure!(
                overlay.try_wait()?.is_none(),
                "Capture overlay exited during startup"
            );
        }
        write_state(path, state)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for StartedCapture {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(overlay) = self.overlay.as_mut() {
                let _ = overlay.kill();
                let _ = overlay.wait();
            }
            let _ = self.recorder.kill();
            let _ = self.recorder.wait();
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    Recording,
    Finalizing,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub phase: Phase,
    recorder: ProcessIdentity,
    overlay: ProcessIdentity,
}

impl Session {
    pub fn stop(&mut self) -> Result<()> {
        if self.phase != Phase::Recording {
            return Ok(());
        }
        let recorder = self.recorder.open()?;
        let overlay = self.overlay.open()?;
        if let Some(process) = &recorder {
            process.signal(libc::SIGINT)?;
        }
        if let Some(process) = &overlay {
            process.signal(libc::SIGTERM)?;
        }
        if let Some(process) = recorder {
            process.wait(5000)?;
        }
        if let Some(process) = overlay {
            process.wait(1000)?;
        }
        self.phase = Phase::Finalizing;
        Ok(())
    }
}

pub fn write_state(path: &Path, state: &impl Serialize) -> Result<()> {
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().context("Session path has no parent")?)?;
    serde_json::to_writer_pretty(&mut file, state)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).context("Cannot publish capture state")?;
    Ok(())
}

pub fn state_path(filename: &str) -> Result<std::path::PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .context("XDG_RUNTIME_DIR is required for capture sessions")?;
    let directory = std::path::PathBuf::from(runtime).join("hyshot");
    if !directory.exists() {
        match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).context("Cannot create capture runtime directory"),
        }
    }
    ensure!(
        !directory.symlink_metadata()?.file_type().is_symlink(),
        "Capture runtime directory must not be a symlink"
    );
    Ok(directory.join(filename))
}

pub struct SessionLock(std::fs::File);
impl SessionLock {
    pub fn acquire(state_path: &Path) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(state_path.with_extension("lock"))?;
        file.try_lock()
            .context("Another capture command is selecting, stopping or finalizing this session")?;
        Ok(Self(file))
    }
}
impl Drop for SessionLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_toggle_is_rejected_until_lock_is_released() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        let guard = SessionLock::acquire(&path).unwrap();
        assert!(SessionLock::acquire(&path).is_err());
        drop(guard);
        assert!(SessionLock::acquire(&path).is_ok());
    }

    #[test]
    fn failed_session_roundtrip_does_not_signal_on_finalize_retry() {
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let identity = ProcessIdentity::capture(child.id()).unwrap();
        let session = Session {
            phase: Phase::Failed,
            recorder: identity.clone(),
            overlay: identity,
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        write_state(&path, &session).unwrap();
        let mut loaded: Session = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let result = loaded.stop();
        let alive = child.try_wait().unwrap().is_none();
        child.kill().unwrap();
        child.wait().unwrap();
        result.unwrap();
        assert!(alive);
        assert_eq!(loaded.phase, Phase::Failed);
    }

    #[test]
    fn abandoned_start_reaps_child() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        drop(StartedCapture {
            recorder: child,
            overlay: None,
            committed: false,
        });
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn failed_state_commit_reaps_children() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        let capture = StartedCapture {
            recorder: child,
            overlay: None,
            committed: false,
        };
        let directory = tempfile::tempdir().unwrap();
        let state_path = directory.path().join("missing/session.json");
        assert!(
            capture
                .commit(&state_path, &serde_json::json!({"pid": pid}))
                .is_err()
        );
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        assert!(!state_path.exists());
    }
}
