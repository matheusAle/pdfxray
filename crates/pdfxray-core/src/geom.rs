use serde::{Deserialize, Serialize};

/// A point in PDF page space (default user space: points, origin bottom-left, y up,
/// before `/Rotate` and `/UserUnit` are applied).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Point {
    /// Creates a point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in page space, normalised so `x0 <= x1` and `y0 <= y1`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x0: f64,
    /// Bottom edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Top edge.
    pub y1: f64,
}

impl Rect {
    /// Creates a rectangle from any two opposite corners.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    /// Width in points.
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// Height in points.
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    /// Area in square points.
    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }

    /// True when `p` lies inside or on the edge.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x0 && p.x <= self.x1 && p.y >= self.y0 && p.y <= self.y1
    }

    /// True when `other` lies entirely inside `self`.
    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x0 >= self.x0 && other.y0 >= self.y0 && other.x1 <= self.x1 && other.y1 <= self.y1
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn new_normalises_corners() {
        let r = Rect::new(10.0, 50.0, 0.0, 20.0);

        assert_eq!(
            r,
            Rect {
                x0: 0.0,
                y0: 20.0,
                x1: 10.0,
                y1: 50.0
            }
        );
        assert_eq!((r.width(), r.height(), r.area()), (10.0, 30.0, 300.0));
    }

    #[test]
    fn contains_includes_edges() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);

        assert!(r.contains(Point::new(10.0, 0.0)));
        assert!(!r.contains(Point::new(10.1, 5.0)));
        assert!(r.contains_rect(&Rect::new(1.0, 1.0, 10.0, 10.0)));
        assert!(!r.contains_rect(&Rect::new(-1.0, 1.0, 5.0, 5.0)));
    }
}
