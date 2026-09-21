//! Linux pidfds bind signals to a process instance, not a reusable PID number.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pid: u32,
    start_ticks: u64,
    boot_id: String,
}

impl ProcessIdentity {
    pub fn capture(pid: u32) -> Result<Self> {
        ensure!(pid > 1, "Invalid capture process ID");
        let start_ticks = read_start(pid)?.context("Capture process exited during startup")?;
        Ok(Self {
            pid,
            start_ticks,
            boot_id: fs::read_to_string("/proc/sys/kernel/random/boot_id")?,
        })
    }

    pub fn open(&self) -> Result<Option<ProcessHandle>> {
        ensure!(self.pid > 1, "Invalid capture process ID");
        if self.boot_id != fs::read_to_string("/proc/sys/kernel/random/boot_id")? {
            return Ok(None);
        }
        // SAFETY: pidfd_open accepts only a PID and zero flags and returns a new fd.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, self.pid, 0u32) };
        if fd < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                return Ok(None);
            }
            return Err(error).context("pidfd_open failed; refusing unsafe PID-only signalling");
        }
        // SAFETY: a successful pidfd_open returns an exclusively owned descriptor.
        let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        if read_start(self.pid)? != Some(self.start_ticks) {
            return Ok(None);
        }
        Ok(Some(ProcessHandle(fd)))
    }
}

fn read_start(pid: u32) -> Result<Option<u64>> {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => Ok(Some(parse_start(&stat)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("Cannot inspect process identity"),
    }
}

fn parse_start(stat: &str) -> Result<u64> {
    stat.rsplit_once(')')
        .context("Malformed process stat")?
        .1
        .split_whitespace()
        .nth(19)
        .context("Missing process start time")?
        .parse()
        .context("Invalid process start time")
}

pub struct ProcessHandle(OwnedFd);

impl ProcessHandle {
    pub fn signal(&self, signal: i32) -> Result<()> {
        // SAFETY: the owned pidfd is valid; a null siginfo is supported by the API.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.0.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0u32,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error).context("Cannot signal capture process");
            }
        }
        Ok(())
    }

    pub fn wait(&self, timeout_ms: i32) -> Result<()> {
        let mut poll = libc::pollfd {
            fd: self.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll points to a valid descriptor structure for one element.
        let result = unsafe { libc::poll(&mut poll, 1, timeout_ms) };
        if result < 0 {
            return Err(io::Error::last_os_error()).context("Waiting for capture process");
        }
        ensure!(
            result > 0 && poll.revents & libc::POLLIN != 0,
            "Capture process has not exited; retaining session for a later stop attempt"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn stale_identity_never_signals_reused_pid() {
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let mut identity = ProcessIdentity::capture(child.id()).unwrap();
        identity.start_ticks += 1;
        let result = identity.open();
        let alive = child.try_wait().unwrap().is_none();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(result.unwrap().is_none());
        assert!(alive);
    }

    #[test]
    fn pidfd_observes_exit_even_before_child_is_reaped() {
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let handle = ProcessIdentity::capture(child.id())
            .unwrap()
            .open()
            .unwrap()
            .unwrap();
        handle.signal(libc::SIGTERM).unwrap();
        let result = handle.wait(1000);
        child.wait().unwrap();
        result.unwrap();
    }

    #[test]
    fn rejects_process_group_and_init_ids() {
        assert!(ProcessIdentity::capture(0).is_err());
        assert!(ProcessIdentity::capture(1).is_err());
    }
}
