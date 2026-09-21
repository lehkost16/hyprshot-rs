//! slurp adapter; the shared rectangle always uses compositor-logical units.
pub use hyshot_core::LogicalRect as Geometry;

pub fn from_slurp_rect(rect: &slurp_rs::Rect) -> anyhow::Result<Geometry> {
    Geometry::new(rect.x, rect.y, rect.width, rect.height)
}
