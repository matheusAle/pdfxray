//! Length units of the rulers and readouts.
//!
//! Everything the core reports is in PDF user-space units. A page's `/UserUnit` scales those to
//! points (1 pt = 1/72 in), and [`Unit`] converts points to what the user asked to see.

use std::borrow::Cow;

use pdfxray_core::Rect;

/// A unit lengths can be shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Unit {
    /// PDF points, 1/72 inch.
    #[default]
    Pt,
    /// Millimetres.
    Mm,
    /// Inches.
    In,
    /// CSS pixels, 1/96 inch.
    Px,
}

impl Unit {
    /// Every unit, in switcher order.
    pub const ALL: [Self; 4] = [Self::Pt, Self::Mm, Self::In, Self::Px];

    /// Short text on the switcher and after numbers, e.g. `mm`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pt => "pt",
            Self::Mm => "mm",
            Self::In => "in",
            Self::Px => "px",
        }
    }

    /// How many of this unit one point is.
    pub const fn per_point(self) -> f64 {
        match self {
            Self::Pt => 1.0,
            Self::Mm => 25.4 / 72.0,
            Self::In => 1.0 / 72.0,
            Self::Px => 96.0 / 72.0,
        }
    }

    /// `points` expressed in this unit.
    pub fn from_points(self, points: f64) -> f64 {
        points * self.per_point()
    }

    /// Digits kept after the decimal point.
    const fn decimals(self) -> usize {
        match self {
            Self::Pt | Self::Mm | Self::Px => 2,
            Self::In => 3,
        }
    }

    /// `points` as a number in this unit without the unit name, e.g. `17.64`.
    pub fn number(self, points: f64) -> String {
        trim(self.from_points(points), self.decimals())
    }

    /// `points` as a number followed by the unit name, e.g. `17.64 mm`.
    pub fn format(self, points: f64) -> String {
        format!("{} {}", self.number(points), self.label())
    }
}

/// Formats lengths of one page: user-space units are scaled by its `/UserUnit` to points and
/// then shown in the chosen [`Unit`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lengths {
    /// The unit to show.
    pub unit: Unit,
    /// Points per user-space unit (`/UserUnit`).
    pub user_unit: f64,
}

impl Lengths {
    /// Lengths in `unit` for a page whose `/UserUnit` is `user_unit`.
    pub const fn new(unit: Unit, user_unit: f64) -> Self {
        Self { unit, user_unit }
    }

    /// A user-space length in points.
    pub fn points(&self, user: f64) -> f64 {
        user * self.user_unit
    }

    /// A user-space length with its unit, e.g. `17.64 mm`.
    pub fn format(&self, user: f64) -> String {
        self.unit.format(self.points(user))
    }

    /// A user-space length without its unit, e.g. `17.64`.
    pub fn number(&self, user: f64) -> String {
        self.unit.number(self.points(user))
    }

    /// A signed user-space offset with its unit; always shows the sign, e.g. `+3 pt`.
    pub fn signed(&self, user: f64) -> String {
        let text = self.format(user);
        if user > 0.0 && !text.starts_with('-') {
            format!("+{text}")
        } else {
            text
        }
    }

    /// Width by height of `rect`, e.g. `30 × 40 pt`.
    pub fn size(&self, rect: Rect) -> String {
        format!("{} × {}", self.number(rect.width()), self.format(rect.height()))
    }
}

/// Rewrites the lengths the core writes in points (`200 pt`, `380 × 280 pt`) in the unit of
/// `lengths`, so one switch in the status bar changes every value in the app. Text without such
/// a length is returned as it is.
pub fn localize<'a>(text: &'a str, lengths: &Lengths) -> Cow<'a, str> {
    const MARK: &str = " pt";
    if lengths.unit == Unit::Pt || !text.contains(MARK) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    let mut changed = false;
    while let Some(at) = find_unit_mark(rest, MARK) {
        let head = &rest[..at];
        if let Some(start) = number_chain_start(head) {
            out.push_str(&head[..start]);
            let numbers: Vec<String> = head[start..]
                .split(" × ")
                .filter_map(|n| n.parse::<f64>().ok())
                .map(|n| lengths.unit.number(n))
                .collect();
            out.push_str(&numbers.join(" × "));
            out.push(' ');
            out.push_str(lengths.unit.label());
            changed = true;
        } else {
            out.push_str(head);
            out.push_str(MARK);
        }
        rest = &rest[at + MARK.len()..];
    }
    if !changed {
        return Cow::Borrowed(text);
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Position of the next `mark` that ends a word, so `200 ptr` is left alone.
fn find_unit_mark(text: &str, mark: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(found) = text[from..].find(mark) {
        let at = from + found;
        let after = &text[at + mark.len()..];
        if !after.starts_with(|c: char| c.is_alphanumeric()) {
            return Some(at);
        }
        from = at + mark.len();
    }
    None
}

/// Where the run of numbers separated by ` × ` that ends `head` starts; `None` when `head` does
/// not end in a number.
fn number_chain_start(head: &str) -> Option<usize> {
    let number_start = |s: &str| -> Option<usize> {
        let start = s
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_digit() || matches!(c, '.' | '-'))
            .last()
            .map(|(i, _)| i)?;
        s[start..].parse::<f64>().ok().map(|_| start)
    };
    let mut start = number_start(head)?;
    while let Some(before) = head[..start].strip_suffix(" × ") {
        match number_start(before) {
            Some(earlier) => start = earlier,
            None => break,
        }
    }
    Some(start)
}

/// `value` with at most `decimals` digits, trailing zeros removed.
fn trim(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text.as_str()
    };
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn units_convert_from_points() {
        assert_eq!(Unit::Pt.format(72.0), "72 pt");
        assert_eq!(Unit::Mm.format(72.0), "25.4 mm");
        assert_eq!(Unit::In.format(72.0), "1 in");
        assert_eq!(Unit::Px.format(72.0), "96 px");
        // Smoke: every unit's factor is consistent with one inch.
        for unit in Unit::ALL {
            assert!((unit.from_points(72.0) - [72.0, 25.4, 1.0, 96.0][unit as usize]).abs() < 1e-9);
        }
    }

    #[test]
    fn numbers_drop_trailing_zeros_and_negative_zero() {
        assert_eq!(Unit::Pt.number(1.5), "1.5");
        assert_eq!(Unit::Pt.number(-0.0001), "0");
        assert_eq!(Unit::Mm.number(100.0), "35.28");
        assert_eq!(Unit::In.number(100.0), "1.389");
        assert_eq!(Unit::Pt.number(0.0), "0");
    }

    #[test]
    fn user_unit_scales_before_the_unit() {
        let lengths = Lengths::new(Unit::In, 2.0);

        assert_eq!(lengths.format(36.0), "1 in");
        assert_eq!(lengths.points(36.0), 72.0);
        assert_eq!(Lengths::new(Unit::Pt, 1.0).signed(3.0), "+3 pt");
        assert_eq!(Lengths::new(Unit::Pt, 1.0).signed(-3.0), "-3 pt");
        assert_eq!(Lengths::new(Unit::Pt, 1.0).signed(0.0), "0 pt");
    }

    #[test]
    fn point_lengths_in_text_follow_the_unit() {
        let mm = Lengths::new(Unit::Mm, 1.0);

        assert_eq!(localize("200 pt", &mm), "70.56 mm");
        assert_eq!(localize("380 × 280 pt", &mm), "134.06 × 98.78 mm");
        assert_eq!(
            localize("Media 200 pt, Crop 100 pt", &mm),
            "Media 70.56 mm, Crop 35.28 mm"
        );
        assert_eq!(localize("200 pt", &Lengths::new(Unit::In, 1.0)), "2.778 in");
        assert_eq!(localize("200 pt", &Lengths::new(Unit::Px, 1.0)), "266.67 px");
        // Smoke: points stay as they are, and so does everything that is not a length.
        assert_eq!(localize("200 pt", &Lengths::new(Unit::Pt, 1.0)), "200 pt");
        assert_eq!(localize("[0 0 200 100]", &mm), "[0 0 200 100]");
        assert_eq!(localize("200 ptr", &mm), "200 ptr");
        assert_eq!(localize("Width pt", &mm), "Width pt");
    }

    #[test]
    fn size_shows_width_then_height() {
        let lengths = Lengths::new(Unit::Pt, 1.0);

        assert_eq!(lengths.size(Rect::new(0.0, 0.0, 30.0, 40.0)), "30 × 40 pt");
    }
}
