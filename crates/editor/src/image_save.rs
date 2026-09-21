use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn save_png(bytes: &[u8], directory: &Path) -> io::Result<PathBuf> {
    if directory.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "configured save directory is empty",
        ));
    }
    fs::create_dir_all(directory)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for suffix in 0.. {
        let name = if suffix == 0 {
            format!("hyshot-{timestamp}.png")
        } else {
            format!("hyshot-{timestamp}-{suffix}.png")
        };
        let path = directory.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes) {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
                return Ok(path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    unreachable!("the filename suffix iterator never ends")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_without_overwriting_existing_output() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("nested");
        let first = save_png(b"first", &output).unwrap();
        let second = save_png(b"second", &output).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(first).unwrap(), b"first");
        assert_eq!(fs::read(second).unwrap(), b"second");
    }

    #[test]
    fn rejects_invalid_destination() {
        assert!(save_png(b"png", Path::new("")).is_err());
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(save_png(b"png", file.path()).is_err());
    }
}
