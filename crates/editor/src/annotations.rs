use gpui::Point;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeType {
    SolidLine,
    DashedLine,
    DottedLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Select,
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Freehand,
    Highlight,
    Mosaic,
    Blur,
    Text,
    Badge,
    Eraser,
}

impl ToolKind {
    pub fn name(&self) -> &'static str {
        match self {
            ToolKind::Select => "Select",
            ToolKind::Rectangle => "Rectangle",
            ToolKind::Ellipse => "Ellipse",
            ToolKind::Line => "StraightLine",
            ToolKind::Arrow => "Arrow",
            ToolKind::Freehand => "Pencil",
            ToolKind::Highlight => "MarkerPen",
            ToolKind::Mosaic => "Mosaic",
            ToolKind::Blur => "Blur",
            ToolKind::Text => "Text",
            ToolKind::Badge => "SerialNumber",
            ToolKind::Eraser => "Eraser",
        }
    }

    pub fn from_config_value(value: &str) -> Option<Self> {
        match value.trim() {
            "Rectangle" => Some(Self::Rectangle),
            "Ellipse" => Some(Self::Ellipse),
            "StraightLine" => Some(Self::Line),
            "Arrow" => Some(Self::Arrow),
            "Pencil" => Some(Self::Freehand),
            "MarkerPen" => Some(Self::Highlight),
            "Mosaic" => Some(Self::Mosaic),
            "Blur" => Some(Self::Blur),
            "Text" => Some(Self::Text),
            "SerialNumber" => Some(Self::Badge),
            "Eraser" => Some(Self::Eraser),
            "Select" => Some(Self::Select),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Annotation {
    Rectangle {
        start: Point<f32>,
        end: Point<f32>,
        color: [u8; 4],
        width: f32,
        stroke_type: StrokeType,
        fill_color: Option<[u8; 4]>,
    },
    Ellipse {
        start: Point<f32>,
        end: Point<f32>,
        color: [u8; 4],
        width: f32,
        stroke_type: StrokeType,
        fill_color: Option<[u8; 4]>,
    },
    Line {
        start: Point<f32>,
        end: Point<f32>,
        color: [u8; 4],
        width: f32,
        stroke_type: StrokeType,
    },
    Arrow {
        start: Point<f32>,
        end: Point<f32>,
        color: [u8; 4],
        width: f32,
    },
    Freehand {
        points: Vec<Point<f32>>,
        color: [u8; 4],
        width: f32,
    },
    Highlight {
        points: Vec<Point<f32>>,
        color: [u8; 4],
        width: f32,
        straight: bool,
    },
    Mosaic {
        rect: [f32; 4], // [min_x, min_y, max_x, max_y]
        block_size: f32,
    },
    Blur {
        rect: [f32; 4], // [min_x, min_y, max_x, max_y]
        sigma: f32,
    },
    Text {
        pos: Point<f32>,
        text: String,
        color: [u8; 4],
        font_size: f32,
    },
    Badge {
        pos: Point<f32>,
        number: usize,
        color: [u8; 4],
        radius: f32,
    },
    Eraser {
        rect: [f32; 4], // [min_x, min_y, max_x, max_y]
    },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum HitTarget {
    TopEdge,
    BottomEdge,
    LeftEdge,
    RightEdge,
    TopLeftCorner,
    TopRightCorner,
    BottomLeftCorner,
    BottomRightCorner,
    Inside,
    Outside,
}

pub trait HitTest {
    fn hit_test(&self, pointer: Point<f32>, stroke_width: f32) -> HitTarget;
}

pub struct RectCoords {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl RectCoords {
    pub fn from_two_points(p1: Point<f32>, p2: Point<f32>) -> Self {
        Self {
            min_x: p1.x.min(p2.x),
            min_y: p1.y.min(p2.y),
            max_x: p1.x.max(p2.x),
            max_y: p1.y.max(p2.y),
        }
    }

    pub fn contains(&self, p: Point<f32>) -> bool {
        p.x >= self.min_x && p.x <= self.max_x && p.y >= self.min_y && p.y <= self.max_y
    }
}

impl HitTest for RectCoords {
    fn hit_test(&self, p: Point<f32>, stroke_width: f32) -> HitTarget {
        let tolerance = 6.0f32.max(stroke_width);
        let half = tolerance / 2.0;

        let mut edges = Vec::new();

        // Top edge: [min_x - half, min_y - half] to [max_x + half, min_y + half]
        if p.x >= self.min_x - half && p.x <= self.max_x + half && p.y >= self.min_y - half && p.y <= self.min_y + half {
            edges.push(HitTarget::TopEdge);
        }
        // Right edge: [max_x - half, min_y - half] to [max_x + half, max_y + half]
        if p.x >= self.max_x - half && p.x <= self.max_x + half && p.y >= self.min_y - half && p.y <= self.max_y + half {
            edges.push(HitTarget::RightEdge);
        }
        // Bottom edge: [min_x - half, max_y - half] to [max_x + half, max_y + half]
        if p.x >= self.min_x - half && p.x <= self.max_x + half && p.y >= self.max_y - half && p.y <= self.max_y + half {
            edges.push(HitTarget::BottomEdge);
        }
        // Left edge: [min_x - half, min_y - half] to [min_x + half, max_y + half]
        if p.x >= self.min_x - half && p.x <= self.min_x + half && p.y >= self.min_y - half && p.y <= self.max_y + half {
            edges.push(HitTarget::LeftEdge);
        }

        if edges.contains(&HitTarget::TopEdge) && edges.contains(&HitTarget::LeftEdge) {
            return HitTarget::TopLeftCorner;
        }
        if edges.contains(&HitTarget::TopEdge) && edges.contains(&HitTarget::RightEdge) {
            return HitTarget::TopRightCorner;
        }
        if edges.contains(&HitTarget::BottomEdge) && edges.contains(&HitTarget::RightEdge) {
            return HitTarget::BottomRightCorner;
        }
        if edges.contains(&HitTarget::BottomEdge) && edges.contains(&HitTarget::LeftEdge) {
            return HitTarget::BottomLeftCorner;
        }

        if let Some(first) = edges.first() {
            return *first;
        }

        if self.contains(p) {
            HitTarget::Inside
        } else {
            HitTarget::Outside
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rect() -> RectCoords {
        RectCoords {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 100.0,
            max_y: 50.0,
        }
    }

    #[test]
    fn test_tool_name_from_config_value() {
        assert_eq!(ToolKind::from_config_value("Rectangle"), Some(ToolKind::Rectangle));
        assert_eq!(ToolKind::from_config_value("  SerialNumber  "), Some(ToolKind::Badge));
        assert_eq!(ToolKind::from_config_value("Blur"), Some(ToolKind::Blur));
        assert_eq!(ToolKind::from_config_value("Eraser"), Some(ToolKind::Eraser));
        assert_eq!(ToolKind::from_config_value("Unknown"), None);
    }

    #[test]
    fn test_hit_test_basic_edges() {
        let rect = test_rect();
        assert_eq!(rect.hit_test(Point::new(50.0, 0.0), 1.0), HitTarget::TopEdge);
        assert_eq!(rect.hit_test(Point::new(50.0, 2.9), 1.0), HitTarget::TopEdge);
        assert_eq!(rect.hit_test(Point::new(50.0, 50.0), 1.0), HitTarget::BottomEdge);
        assert_eq!(rect.hit_test(Point::new(0.0, 25.0), 1.0), HitTarget::LeftEdge);
        assert_eq!(rect.hit_test(Point::new(100.0, 25.0), 1.0), HitTarget::RightEdge);
    }

    #[test]
    fn test_hit_test_corners() {
        let rect = test_rect();
        assert_eq!(rect.hit_test(Point::new(0.0, 0.0), 1.0), HitTarget::TopLeftCorner);
        assert_eq!(rect.hit_test(Point::new(100.0, 0.0), 1.0), HitTarget::TopRightCorner);
        assert_eq!(rect.hit_test(Point::new(0.0, 50.0), 1.0), HitTarget::BottomLeftCorner);
        assert_eq!(rect.hit_test(Point::new(100.0, 50.0), 1.0), HitTarget::BottomRightCorner);
    }

    #[test]
    fn test_hit_test_inside_outside() {
        let rect = test_rect();
        assert_eq!(rect.hit_test(Point::new(50.0, 25.0), 1.0), HitTarget::Inside);
        assert_eq!(rect.hit_test(Point::new(-10.0, 25.0), 1.0), HitTarget::Outside);
        assert_eq!(rect.hit_test(Point::new(50.0, -10.0), 1.0), HitTarget::Outside);
        assert_eq!(rect.hit_test(Point::new(150.0, 25.0), 1.0), HitTarget::Outside);
    }
}
