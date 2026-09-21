use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn save_png(bytes: &[u8]) -> io::Result<PathBuf> {
    let directory = default_directory()?;
    fs::create_dir_all(&directory)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for suffix in 0.. {
        let name = if suffix == 0 {
            format!("annotator-{timestamp}.png")
        } else {
            format!("annotator-{timestamp}-{suffix}.png")
        };
        let path = directory.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(bytes)?;
                return Ok(path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    unreachable!("the filename suffix iterator never ends")
}

fn default_directory() -> io::Result<PathBuf> {
    let directory = PathBuf::from(crate::config::AnnotatorConfig::load().save_directory);
    if directory.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "configured save directory is empty",
        ));
    }
    Ok(directory)
}
