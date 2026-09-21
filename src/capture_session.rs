//! Shared recorder/overlay lifecycle for video recording and scrolling capture.

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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

    pub fn recorder_pid(&self) -> u32 {
        self.recorder.id()
    }
    pub fn overlay_pid(&self) -> u32 {
        self.overlay.as_ref().expect("overlay started").id()
    }

    pub fn commit(mut self, path: &Path, state: &impl Serialize) -> Result<()> {
        ensure!(
            self.recorder.try_wait()?.is_none(),
            "Recorder exited before session was ready"
        );
        let parent = path.parent().context("Session path has no parent")?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, state)?;
        file.flush()?;
        file.persist(path)
            .context("Failed to commit capture session")?;
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

fn is_running(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

pub fn stop(recorder_pid: u32, overlay_pid: u32) -> Result<()> {
    ensure!(
        recorder_pid > 1 && overlay_pid > 1,
        "Invalid capture process IDs"
    );
    if is_running(recorder_pid) {
        let status = Command::new("kill")
            .args(["-2", &recorder_pid.to_string()])
            .status()?;
        ensure!(
            status.success() || !is_running(recorder_pid),
            "Failed to stop recorder"
        );
    }
    if is_running(overlay_pid) {
        let _ = Command::new("kill").arg(overlay_pid.to_string()).status();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while is_running(recorder_pid) {
        ensure!(
            Instant::now() < deadline,
            "Recorder has not exited; retaining session state. Retry stopping before processing the video."
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_process_group_ids() {
        assert!(stop(0, 20).is_err());
        assert!(stop(20, 1).is_err());
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
        assert!(!is_running(pid));
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
        assert!(!is_running(pid));
        assert!(!state_path.exists());
    }
}
