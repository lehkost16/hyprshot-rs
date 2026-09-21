//! Pixel data and coordinate contracts shared by capture and editing.
//! No compositor, UI, process management or configuration persistence.

mod document;
mod geometry;

pub use document::{ImageDocument, ImageSource, MAX_IMAGE_BYTES};
pub use geometry::LogicalRect;
