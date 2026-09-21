use anyhow::{Context, Result, ensure};
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

pub struct FrameReader {
    child: Child,
    stdout: ChildStdout,
    stderr: tempfile::NamedTempFile,
    frame_bytes: usize,
    finished: bool,
}

impl FrameReader {
    pub fn open(path: &Path, width: usize, height: usize, fps: u32) -> Result<Self> {
        let frame_bytes = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(3))
            .context("Decoded frame dimensions overflow")?;
        ensure!(
            frame_bytes > 0 && frame_bytes <= MAX_FRAME_BYTES,
            "Decoded frame exceeds the 64 MiB RGB frame limit"
        );
        let stderr = tempfile::NamedTempFile::new()?;
        let mut child = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-noautorotate", "-i"])
            .arg(path)
            .args(["-map", "0:v:0", "-an", "-sn", "-vf"])
            .arg(format!("fps={}", fps.max(1)))
            // Let ffmpeg handle the source color matrix/range instead of assuming BT.601.
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr.reopen()?))
            .spawn()
            .context("Failed to start ffmpeg decoder")?;
        let stdout = child.stdout.take().context("Missing decoder stdout")?;
        Ok(Self {
            child,
            stdout,
            stderr,
            frame_bytes,
            finished: false,
        })
    }

    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        if self.finished {
            return Ok(None);
        }
        let mut bytes = vec![0; self.frame_bytes];
        if read_frame(&mut self.stdout, &mut bytes)? {
            return Ok(Some(bytes));
        }
        let status = self.child.wait()?;
        self.finished = true;
        let mut error = String::new();
        self.stderr
            .reopen()?
            .take(8192)
            .read_to_string(&mut error)?;
        ensure!(status.success(), "ffmpeg decode failed: {}", error.trim());
        Ok(None)
    }
}

impl Drop for FrameReader {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn read_frame(reader: &mut impl Read, bytes: &mut [u8]) -> Result<bool> {
    let mut read = 0;
    while read < bytes.len() {
        match reader.read(&mut bytes[read..]) {
            Ok(0) if read == 0 => return Ok(false),
            Ok(0) => anyhow::bail!("Truncated decoded frame: {read}/{} bytes", bytes.len()),
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error).context("Reading decoded frame"),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eof_and_truncated_frame_are_distinct() {
        let mut output = [0; 6];
        assert!(!read_frame(&mut &b""[..], &mut output).unwrap());
        assert!(read_frame(&mut &b"abc"[..], &mut output).is_err());
        assert!(read_frame(&mut &b"abcdef"[..], &mut output).unwrap());
        assert_eq!(&output, b"abcdef");
    }
}
