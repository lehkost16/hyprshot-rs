//! Typed geometry used across capture/trim/save to avoid repeated string parsing.

use anyhow::{Context, Result};
use std::fmt;
use std::str::FromStr;

/// Compositor-global logical coordinates. Never use this as decoded pixel dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl LogicalRect {
    pub fn contains(&self, other: &Self) -> bool {
        i64::from(other.x) >= i64::from(self.x)
            && i64::from(other.y) >= i64::from(self.y)
            && i64::from(other.x) + i64::from(other.width)
                <= i64::from(self.x) + i64::from(self.width)
            && i64::from(other.y) + i64::from(other.height)
                <= i64::from(self.y) + i64::from(self.height)
    }

    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Result<Self> {
        if width <= 0 || height <= 0 {
            return Err(anyhow::anyhow!(
                "Invalid geometry dimensions: width={} or height={} is non-positive",
                width,
                height
            ));
        }
        x.checked_add(width)
            .context("Geometry right edge overflows")?;
        y.checked_add(height)
            .context("Geometry bottom edge overflows")?;
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
}

impl FromStr for LogicalRect {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let input = s.trim();
        if input.is_empty() {
            return Err(anyhow::anyhow!("Invalid geometry format: empty string"));
        }

        let mut parts = input.split_whitespace();
        let xy = parts
            .next()
            .context("Invalid geometry format: missing coordinates")?;
        let wh = parts
            .next()
            .context("Invalid geometry format: missing dimensions")?;
        if parts.next().is_some() {
            return Err(anyhow::anyhow!(
                "Invalid geometry format: expected 'x,y wxh', got '{}'",
                input
            ));
        }

        let mut xy_parts = xy.split(',');
        let x: i32 = xy_parts
            .next()
            .context("Invalid geometry format: missing x")?
            .parse()
            .context("Invalid x coordinate")?;
        let y: i32 = xy_parts
            .next()
            .context("Invalid geometry format: missing y")?
            .parse()
            .context("Invalid y coordinate")?;
        if xy_parts.next().is_some() {
            return Err(anyhow::anyhow!(
                "Invalid geometry format: expected 'x,y wxh', got '{}'",
                input
            ));
        }

        let mut wh_parts = wh.split('x');
        let width: i32 = wh_parts
            .next()
            .context("Invalid geometry format: missing width")?
            .parse()
            .context("Invalid width")?;
        let height: i32 = wh_parts
            .next()
            .context("Invalid geometry format: missing height")?
            .parse()
            .context("Invalid height")?;
        if wh_parts.next().is_some() {
            return Err(anyhow::anyhow!(
                "Invalid geometry format: expected 'x,y wxh', got '{}'",
                input
            ));
        }

        LogicalRect::new(x, y, width, height)
    }
}

impl fmt::Display for LogicalRect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{} {}x{}", self.x, self.y, self.width, self.height)
    }
}
