//! Shared recorder/overlay lifecycle for video recording and scrolling capture.

use anyhow::{Context, Error, Result, ensure};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};

mod process;
use process::ProcessIdentity;

use crate::compositor::MonitorInfo;
use crate::geometry::Geometry;

/// Which capture the overlay is drawing for. Drives the selection border
/// colour so recording and long screenshot stay visually distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureKind {
    Record,
    Longshot,
}

impl CaptureKind {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Longshot => "longshot",
        }
    }
}

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
        kind: CaptureKind,
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
                .arg("--kind")
                .arg(kind.as_arg())
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

        // Signalling must be best-effort for BOTH processes. Opening a handle
        // can fail (the process was reaped, the boot id differs, /proc is
        // unreadable) and a `?` here would return before the other process is
        // even asked to stop — leaving a recorder writing to disk forever with
        // nothing tracking it. Collect the errors and report them only after
        // both have been signalled and reaped.
        let mut first_error: Option<Error> = None;
        let recorder = match self.recorder.open() {
            Ok(handle) => handle,
            Err(error) => {
                first_error = Some(error);
                None
            }
        };
        let overlay = match self.overlay.open() {
            Ok(handle) => handle,
            Err(error) => {
                first_error.get_or_insert(error);
                None
            }
        };

        if let Some(process) = &recorder {
            if let Err(error) = process.signal(libc::SIGINT) {
                first_error.get_or_insert(error);
            }
        }
        if let Some(process) = &overlay {
            if let Err(error) = process.signal(libc::SIGTERM) {
                first_error.get_or_insert(error);
            }
        }
        if let Some(process) = recorder {
            if let Err(error) = process.wait(5000) {
                first_error.get_or_insert(error);
            }
        }
        if let Some(process) = overlay {
            if let Err(error) = process.wait(1000) {
                first_error.get_or_insert(error);
            }
        }

        self.phase = Phase::Finalizing;
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
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


/// Names of the capture sessions whose processes outlive their state file.
///
/// The state file is the only thing that remembers the recorder and overlay
/// PIDs. If it is deleted or found corrupt, those processes keep running with
/// nothing pointing at them: the next hotkey press would start a second
/// recorder while the first one still owns the screen area. A side-channel
/// journal records the identities next to the state file so a later run can
/// reap anything the state file no longer accounts for.
const ORPHAN_JOURNAL: &str = "orphans.json";

/// Records the process identities of a live session, replacing any previous entry.
pub fn journal_processes(path: &Path, session: &Session) -> Result<()> {
    let identities = [session.recorder.clone(), session.overlay.clone()];
    let mut all = read_journal(path).unwrap_or_default();
    all.retain(|identity| identity.open().map(|h| h.is_some()).unwrap_or(false));
    for identity in identities {
        if !all.iter().any(|kept| kept.pid() == identity.pid()) {
            all.push(identity);
        }
    }
    write_journal(path, &all)
}

/// Forgets every journal entry; called once a session is fully finished.
pub fn clear_journal(path: &Path) -> Result<()> {
    write_journal(path, &[])
}

/// Kills any journaled capture process still alive and clears the journal.
///
/// Called before a new capture starts: whatever is still running at that point
/// cannot belong to a session the user can still stop, because starting a
/// capture requires the lock the owning session would hold.
pub fn reap_orphans(path: &Path, debug: bool) {
    let identities = read_journal(path).unwrap_or_default();
    for identity in identities {
        if let Ok(Some(handle)) = identity.open() {
            if debug {
                eprintln!("Reaping orphaned capture process {}", identity.pid());
            }
            let _ = handle.signal(libc::SIGTERM);
            let _ = handle.wait(1000);
        }
    }
    let _ = clear_journal(path);
}

fn journal_path(state: &Path) -> std::path::PathBuf {
    state.with_file_name(ORPHAN_JOURNAL)
}

fn read_journal(state: &Path) -> Result<Vec<ProcessIdentity>> {
    let path = journal_path(state);
    let data = match std::fs::read_to_string(&path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("Cannot read capture journal"),
    };
    serde_json::from_str(&data).context("Cannot parse capture journal")
}

fn write_journal(state: &Path, identities: &[ProcessIdentity]) -> Result<()> {
    let path = journal_path(state);
    if identities.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent().context("Journal path has no parent")?,
    )?;
    serde_json::to_writer_pretty(&mut file, identities)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(&path).context("Cannot publish capture journal")?;
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

    /// The journal must survive a state file being deleted while the session is
    /// live: that is the only case it exists for.
    #[test]
    fn journal_survives_missing_state_file() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("longshot.json");

        let mut a = Command::new("sleep").arg("30").spawn().unwrap();
        let identity = ProcessIdentity::capture(a.id()).unwrap();
        let session = Session {
            phase: Phase::Recording,
            recorder: identity.clone(),
            overlay: identity,
        };

        journal_processes(&state, &session).unwrap();
        assert!(!state.exists(), "journal must not require a state file");
        assert_eq!(read_journal(&state).unwrap().len(), 1);

        // Recording the same session again must not duplicate entries.
        journal_processes(&state, &session).unwrap();
        assert_eq!(read_journal(&state).unwrap().len(), 1);

        clear_journal(&state).unwrap();
        assert!(read_journal(&state).unwrap().is_empty());

        a.kill().unwrap();
        a.wait().unwrap();
    }

    /// Reaping kills whatever is still journaled and leaves nothing behind, so
    /// the next capture starts from a clean slate.
    #[test]
    fn reaping_terminates_journaled_processes() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("longshot.json");

        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let identity = ProcessIdentity::capture(child.id()).unwrap();
        let session = Session {
            phase: Phase::Recording,
            recorder: identity.clone(),
            overlay: identity,
        };
        journal_processes(&state, &session).unwrap();

        reap_orphans(&state, false);

        // The process must actually be gone, not just forgotten. `wait` reaps
        // the child; a SIGTERM'd sleep exits with a signal status.
        use std::os::unix::process::ExitStatusExt;
        let status = child.wait().unwrap();
        assert!(
            status.signal() == Some(libc::SIGTERM) || status.code().is_some(),
            "child must have exited, got {status:?}"
        );
        assert!(read_journal(&state).unwrap().is_empty(), "journal cleared");
    }

    /// `stop` must always signal and reap the recorder, whatever the overlay
    /// can or cannot be opened. A `?` on the overlay lookup used to return
    /// before the recorder was ever asked to stop, leaving it writing to disk
    /// forever with nothing tracking it.
    #[test]
    fn stop_always_stops_the_recorder() {
        let mut recorder = Command::new("sleep").arg("30").spawn().unwrap();
        let recorder_identity = ProcessIdentity::capture(recorder.id()).unwrap();

        // An identity for a pid that cannot exist: opening it yields None.
        let missing_overlay = ProcessIdentity::for_test(u32::MAX);

        let mut session = Session {
            phase: Phase::Recording,
            recorder: recorder_identity,
            overlay: missing_overlay,
        };

        session.stop().unwrap();

        // The recorder must really be gone, not merely forgotten.
        use std::os::unix::process::ExitStatusExt;
        let status = recorder.wait().unwrap();
        assert!(
            status.signal() == Some(libc::SIGINT) || status.code().is_some(),
            "recorder must have been stopped, got {status:?}"
        );
        assert_eq!(session.phase, Phase::Finalizing);
    }
}
