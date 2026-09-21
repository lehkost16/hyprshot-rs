use anyhow::{Context, Result, ensure};

pub const MAX_CANVAS_BYTES: usize = 128 * 1024 * 1024;

pub struct Canvas {
    pixels: Vec<u8>,
    width: usize,
    origin: i64,
    max_bytes: usize,
}

impl Canvas {
    pub fn new(width: usize, max_bytes: usize) -> Result<Self> {
        ensure!(
            width > 0 && width <= u32::MAX as usize,
            "Invalid canvas width"
        );
        Ok(Self {
            pixels: Vec::new(),
            width,
            origin: 0,
            max_bytes,
        })
    }

    pub fn height(&self) -> usize {
        self.pixels.len() / (self.width * 3)
    }
    pub fn origin(&self) -> i64 {
        self.origin
    }
    pub fn end(&self) -> i64 {
        self.origin + self.height() as i64
    }

    pub fn place(&mut self, rgb: &[u8], y: i64) -> Result<bool> {
        let row_bytes = self.width.checked_mul(3).context("Canvas row overflow")?;
        ensure!(
            rgb.len().is_multiple_of(row_bytes),
            "Invalid RGB row length"
        );
        let height = rgb.len() / row_bytes;
        ensure!(height > 0, "Empty canvas placement");
        let frame_end = y
            .checked_add(height as i64)
            .context("Canvas position overflow")?;
        let origin = if self.pixels.is_empty() {
            y
        } else {
            self.origin.min(y)
        };
        let end = if self.pixels.is_empty() {
            frame_end
        } else {
            self.end().max(frame_end)
        };
        let new_height =
            usize::try_from(end.checked_sub(origin).context("Canvas extent overflow")?)?;
        let new_bytes = row_bytes
            .checked_mul(new_height)
            .context("Canvas allocation overflow")?;
        ensure!(
            new_height <= u32::MAX as usize && new_bytes <= self.max_bytes,
            "Long screenshot exceeds its canvas memory limit; source recording has not been discarded"
        );
        let grew = new_bytes > self.pixels.len();
        if new_bytes > self.pixels.len() {
            self.pixels
                .try_reserve_exact(new_bytes - self.pixels.len())
                .context("Unable to grow long screenshot")?;
            let old_len = self.pixels.len();
            self.pixels.resize(new_bytes, 0);
            if origin < self.origin && old_len > 0 {
                let shift = (self.origin - origin) as usize * row_bytes;
                self.pixels.copy_within(0..old_len, shift);
            }
        }
        self.origin = origin;
        let offset = usize::try_from(y - origin)? * row_bytes;
        self.pixels[offset..offset + rgb.len()].copy_from_slice(rgb);
        Ok(grew)
    }

    pub fn into_image(self) -> Result<image::RgbImage> {
        image::RgbImage::from_raw(self.width as u32, self.height() as u32, self.pixels)
            .context("Invalid longshot canvas")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appending_and_prepending_preserve_rows() {
        let mut canvas = Canvas::new(1, 30).unwrap();
        canvas.place(&[2; 6], 0).unwrap();
        canvas.place(&[1; 3], -1).unwrap();
        canvas.place(&[3; 3], 2).unwrap();
        assert_eq!(
            canvas.into_image().unwrap().as_raw(),
            &[1, 1, 1, 2, 2, 2, 2, 2, 2, 3, 3, 3]
        );
    }
    #[test]
    fn exceeding_limit_is_an_error_not_silent_truncation() {
        let mut canvas = Canvas::new(1, 6).unwrap();
        canvas.place(&[1; 6], 0).unwrap();
        assert!(canvas.place(&[2; 3], 2).is_err());
        assert_eq!(canvas.height(), 2);
    }
}
