use anyhow::{Context, Result, ensure};
use image::{ImageReader, RgbaImage};
use std::io::{BufRead, Cursor, Seek};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::LogicalRect;

pub const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    Capture(LogicalRect),
    File(PathBuf),
    Stitched(PathBuf),
}

/// Immutable original pixels. View zoom and annotations never modify this buffer.
#[derive(Debug, Clone)]
pub struct ImageDocument {
    pixels: Arc<RgbaImage>,
    source: ImageSource,
}

impl ImageDocument {
    pub fn new(pixels: RgbaImage, source: ImageSource) -> Result<Self> {
        validate_size(pixels.width(), pixels.height())?;
        if let ImageSource::Capture(region) = &source {
            LogicalRect::new(region.x, region.y, region.width, region.height)?;
        }
        Ok(Self {
            pixels: Arc::new(pixels),
            source,
        })
    }

    pub fn from_png(bytes: &[u8], source: ImageSource) -> Result<Self> {
        Self::decode(
            ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png),
            source,
        )
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        Self::from_file_with_budget(path, MAX_IMAGE_BYTES)
    }

    pub fn from_file_with_budget(path: &Path, remaining_bytes: u64) -> Result<Self> {
        let reader = ImageReader::open(path)
            .with_context(|| format!("Failed to open image {}", path.display()))?
            .with_guessed_format()?;
        Self::decode_with_budget(reader, ImageSource::File(path.to_owned()), remaining_bytes)
    }

    fn decode<R: BufRead + Seek>(reader: ImageReader<R>, source: ImageSource) -> Result<Self> {
        Self::decode_with_budget(reader, source, MAX_IMAGE_BYTES)
    }

    fn decode_with_budget<R: BufRead + Seek>(
        mut reader: ImageReader<R>,
        source: ImageSource,
        budget: u64,
    ) -> Result<Self> {
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(MAX_IMAGE_BYTES.min(budget));
        reader.limits(limits);
        // Validate the eventual RGBA allocation before converting RGB/gray input.
        let decoder = reader
            .into_decoder()
            .context("Failed to decode image header")?;
        use image::ImageDecoder;
        let (width, height) = decoder.dimensions();
        validate_size(width, height)?;
        ensure!(
            u64::from(width) * u64::from(height) * 4 <= budget,
            "Images exceed the shared editor RGBA memory budget"
        );
        let pixels = image::DynamicImage::from_decoder(decoder)?.into_rgba8();
        Self::new(pixels, source)
    }

    pub fn pixels(&self) -> &Arc<RgbaImage> {
        &self.pixels
    }
    pub fn source(&self) -> &ImageSource {
        &self.source
    }
    pub fn pixel_size(&self) -> (u32, u32) {
        self.pixels.dimensions()
    }

    /// Actual decoded pixel-to-logical ratio, not the UI's display scale.
    pub fn capture_pixel_scale(&self) -> Option<(f64, f64)> {
        match &self.source {
            ImageSource::Capture(region) => Some((
                self.pixels.width() as f64 / region.width as f64,
                self.pixels.height() as f64 / region.height as f64,
            )),
            _ => None,
        }
    }
}

fn validate_size(width: u32, height: u32) -> Result<()> {
    ensure!(width > 0 && height > 0, "Image is empty");
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(4))
        .context("Image dimensions overflow")?;
    ensure!(
        bytes <= MAX_IMAGE_BYTES,
        "Image exceeds the 256 MiB decoded RGBA limit; refusing to resize or exhaust memory"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_pixels_and_fractional_capture_scale_remain_independent() {
        let pixels = RgbaImage::from_pixel(150, 75, image::Rgba([23, 78, 190, 112]));
        let region = LogicalRect::new(-100, 0, 100, 50).unwrap();
        let document = ImageDocument::new(pixels.clone(), ImageSource::Capture(region)).unwrap();
        assert_eq!(document.pixel_size(), (150, 75));
        assert_eq!(document.capture_pixel_scale(), Some((1.5, 1.5)));
        assert_eq!(document.pixels().as_ref(), &pixels);
        assert!(Arc::ptr_eq(document.pixels(), document.clone().pixels()));
    }

    #[test]
    fn rejects_excessive_dimensions_before_rgba_allocation() {
        assert!(validate_size(u32::MAX, u32::MAX).is_err());
        assert!(validate_size(100_000, 100_000).is_err());
        assert!(validate_size(0, 10).is_err());
    }

    #[test]
    fn file_decode_respects_remaining_multi_image_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        RgbaImage::from_pixel(10, 10, image::Rgba([12, 34, 56, 255]))
            .save(&path)
            .unwrap();
        assert!(ImageDocument::from_file_with_budget(&path, 399).is_err());
        assert!(ImageDocument::from_file(&path).is_ok());
    }
}
