//! The six-number affine matrix of PDF (`a b c d e f`), its arithmetic and a decomposition into
//! the quantities a person reads: scale, rotation, skew and whether the scaling is uniform.

use serde::Serialize;

use crate::Point;

/// Largest difference between the two scale factors still called uniform.
const UNIFORM_TOLERANCE: f64 = 1e-6;
/// Scale factors smaller than this are treated as zero (a collapsed axis).
const DEGENERATE: f64 = 1e-12;

/// A PDF transformation matrix `[a b c d e f]`.
///
/// Points are row vectors, as in ISO 32000-2 §8.3.4: `x' = a·x + c·y + e` and
/// `y' = b·x + d·y + f`. `a` and `d` scale, `b` and `c` skew (or rotate), `e` and `f` translate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Matrix {
    /// Horizontal scale.
    pub a: f64,
    /// Vertical skew.
    pub b: f64,
    /// Horizontal skew.
    pub c: f64,
    /// Vertical scale.
    pub d: f64,
    /// Horizontal translation.
    pub e: f64,
    /// Vertical translation.
    pub f: f64,
}

/// A [`Matrix`] taken apart into scale, rotation and skew.
///
/// The matrix is read as: scale along its own axes, shear the second axis, rotate. Angles are
/// counter-clockwise in PDF's y-up space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Decomposed {
    /// Length of the transformed x axis. Always positive.
    pub scale_x: f64,
    /// Scale along the second axis, negative when the matrix mirrors (negative determinant).
    pub scale_y: f64,
    /// Angle of the transformed x axis in degrees, in `(-180, 180]`.
    pub rotation_deg: f64,
    /// How far the second axis leans away from perpendicular, in degrees (0 when the axes stay
    /// at a right angle).
    pub skew_deg: f64,
    /// True when `|scale_x|` and `|scale_y|` are equal (to within one part in a million).
    pub uniform: bool,
    /// `|scale_x| / |scale_y|`; `0` when an axis collapsed to nothing.
    pub ratio: f64,
}

impl Matrix {
    /// The identity matrix.
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// The meaning of operand `index` (0 = `a` ... 5 = `f`) when the matrix is a `cm`: the label
    /// the inspector shows beside each number. Out-of-range indices give `"?"`.
    pub fn meaning(index: usize) -> &'static str {
        [
            "scale x",
            "skew y",
            "skew x",
            "scale y",
            "translate x",
            "translate y",
        ]
        .get(index)
        .copied()
        .unwrap_or("?")
    }

    /// Where `p` lands.
    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    /// The matrix that applies `self` first and `other` second (`self × other` in PDF's row-vector
    /// convention). A `cm` operator turns the CTM `ctm` into `cm.concat(&ctm)`.
    #[must_use]
    pub fn concat(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    /// The six numbers in `a b c d e f` order.
    pub fn to_array(&self) -> [f64; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }

    /// True for the identity matrix (to within a part in a million).
    pub fn is_identity(&self) -> bool {
        self.to_array()
            .iter()
            .zip(Self::IDENTITY.to_array())
            .all(|(v, want)| (v - want).abs() < UNIFORM_TOLERANCE)
    }

    /// Splits the linear part into scale, rotation and skew. See [`Decomposed`].
    pub fn decompose(&self) -> Decomposed {
        let scale_x = self.a.hypot(self.b);
        let det = self.a * self.d - self.b * self.c;
        let (scale_y, skew_deg) = if scale_x < DEGENERATE {
            (self.c.hypot(self.d), 0.0)
        } else {
            let shear = self.a * self.c + self.b * self.d;
            (det / scale_x, shear.atan2(det.abs()).to_degrees())
        };
        let rotation_deg = if scale_x < DEGENERATE {
            0.0
        } else {
            self.b.atan2(self.a).to_degrees()
        };
        let collapsed = scale_x < DEGENERATE || scale_y.abs() < DEGENERATE;
        let ratio = if collapsed { 0.0 } else { scale_x / scale_y.abs() };
        Decomposed {
            scale_x,
            scale_y,
            rotation_deg,
            skew_deg,
            uniform: !collapsed && (ratio - 1.0).abs() < UNIFORM_TOLERANCE,
            ratio,
        }
    }
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl From<[f64; 6]> for Matrix {
    fn from([sx, ky, kx, sy, tx, ty]: [f64; 6]) -> Self {
        Self {
            a: sx,
            b: ky,
            c: kx,
            d: sy,
            e: tx,
            f: ty,
        }
    }
}

impl From<Matrix> for [f64; 6] {
    fn from(m: Matrix) -> Self {
        m.to_array()
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the operands are exactly representable and the arithmetic exact"
)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_design_example_is_non_uniform_with_ratio_2_27() {
        let d = Matrix::from([340.0, 0.0, 0.0, 150.0, 40.0, 222.0]).decompose();

        assert_eq!(
            (d.scale_x, d.scale_y, d.rotation_deg, d.skew_deg, d.uniform),
            (340.0, 150.0, 0.0, 0.0, false)
        );
        assert_eq!(format!("{:.2}", d.ratio), "2.27");
    }

    #[test]
    fn a_quarter_turn_is_rotation_90_and_uniform() {
        let d = Matrix::from([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]).decompose();

        assert_eq!(
            (
                d.scale_x,
                d.scale_y,
                d.rotation_deg,
                d.skew_deg,
                d.uniform,
                d.ratio
            ),
            (1.0, 1.0, 90.0, 0.0, true, 1.0)
        );
    }

    #[test]
    fn a_scaled_rotation_keeps_its_scale_and_angle() {
        let (s, c) = 30f64.to_radians().sin_cos();
        let d = Matrix::from([2.0 * c, 2.0 * s, -2.0 * s, 2.0 * c, 5.0, 6.0]).decompose();

        assert!(close(d.scale_x, 2.0) && close(d.scale_y, 2.0), "{d:?}");
        assert!(close(d.rotation_deg, 30.0), "{d:?}");
        assert!(close(d.skew_deg, 0.0), "{d:?}");
        assert!(d.uniform);
    }

    #[test]
    fn a_horizontal_shear_reports_its_angle() {
        let d = Matrix::from([1.0, 0.0, 1.0, 1.0, 0.0, 0.0]).decompose();

        assert!(close(d.skew_deg, 45.0), "{d:?}");
        assert!(close(d.rotation_deg, 0.0), "{d:?}");
        assert!(d.uniform, "shear alone does not change the axis scales");
    }

    #[test]
    fn a_mirror_has_a_negative_second_scale() {
        let d = Matrix::from([1.0, 0.0, 0.0, -2.0, 0.0, 0.0]).decompose();

        assert_eq!(
            (d.scale_x, d.scale_y, d.ratio, d.uniform),
            (1.0, -2.0, 0.5, false)
        );
    }

    #[test]
    fn a_collapsed_matrix_is_not_uniform_and_has_ratio_zero() {
        let d = Matrix::from([0.0; 6]).decompose();

        assert_eq!((d.scale_x, d.scale_y, d.ratio, d.uniform), (0.0, 0.0, 0.0, false));
        let flat = Matrix::from([3.0, 0.0, 0.0, 0.0, 0.0, 0.0]).decompose();
        assert_eq!((flat.scale_x, flat.ratio, flat.uniform), (3.0, 0.0, false));
    }

    #[test]
    fn apply_follows_the_row_vector_convention() {
        let m = Matrix::from([2.0, 0.0, 0.0, 3.0, 10.0, 20.0]);

        assert_eq!(m.apply(Point::new(1.0, 1.0)), Point::new(12.0, 23.0));
        assert_eq!(
            Matrix::from([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]).apply(Point::new(1.0, 0.0)),
            Point::new(0.0, 1.0)
        );
    }

    #[test]
    fn concat_applies_self_first() {
        let scale = Matrix::from([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
        let shift = Matrix::from([1.0, 0.0, 0.0, 1.0, 10.0, 0.0]);
        let p = Point::new(1.0, 1.0);

        assert_eq!(scale.concat(&shift).apply(p), Point::new(12.0, 2.0));
        assert_eq!(shift.concat(&scale).apply(p), Point::new(22.0, 2.0));
        assert_eq!(
            scale.concat(&shift).to_array(),
            [2.0, 0.0, 0.0, 2.0, 10.0, 0.0],
            "scale then shift"
        );
        assert_eq!(scale.concat(&Matrix::IDENTITY), scale);
    }

    #[test]
    fn meanings_follow_the_cm_operand_order() {
        let labels: Vec<&str> = (0..7).map(Matrix::meaning).collect();

        assert_eq!(
            labels,
            [
                "scale x",
                "skew y",
                "skew x",
                "scale y",
                "translate x",
                "translate y",
                "?"
            ]
        );
    }

    #[test]
    fn arrays_convert_both_ways() {
        let m = Matrix::from([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

        assert_eq!(<[f64; 6]>::from(m), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert!(Matrix::default().is_identity());
        assert!(!m.is_identity());
    }
}
