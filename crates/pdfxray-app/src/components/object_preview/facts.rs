//! What a preview card says in words: the line of its header, the chips of its footer, and the
//! rows the inspector lists under it.

use pdfxray_core::{
    ColorSpaceVisual, FontProgram, FontVisual, ImageVisual, ShadingStop, ShadingVisual, Visual, VisualKind,
    num,
};

/// The standard 14 fonts: a viewer has them, so a file may leave them out.
const STANDARD_14: [&str; 14] = [
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

/// One fact shown with a preview, as a row under the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// `Pixels`.
    pub key: &'static str,
    /// `2 × 2`.
    pub value: String,
    /// Shown in the warning colour.
    pub warn: bool,
}

impl Fact {
    fn new(key: &'static str, value: impl Into<String>) -> Self {
        Self {
            key,
            value: value.into(),
            warn: false,
        }
    }

    const fn warn(mut self, warn: bool) -> Self {
        self.warn = warn;
        self
    }
}

/// A chip of a card's footer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipFact {
    /// `DeviceRGB`.
    pub text: String,
    /// An amber chip with a warning icon.
    pub warn: bool,
}

impl ChipFact {
    fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            warn: false,
        }
    }

    fn warning(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            warn: true,
        }
    }
}

/// The words of a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// The header line of the centre card: `2 × 2 px · 8 bpc · DeviceRGB`.
    pub meta: String,
    /// The footer of the centre card.
    pub chips: Vec<ChipFact>,
    /// The one chip of the inspector card.
    pub chip: ChipFact,
}

/// The header line and the chips of a card of `visual`.
pub fn summary(visual: &Visual) -> Summary {
    match &visual.kind {
        VisualKind::Image(image) => image_summary(image),
        VisualKind::Font(font) => font_summary(font),
        VisualKind::Form(form) => {
            let a = &form.area;
            let area = format!("{} × {} pt", num(a.width()), num(a.height()));
            let mut chips = vec![ChipFact::new("Form XObject"), ChipFact::new(area.clone())];
            if let Some(page) = form.borrows_from {
                chips.push(ChipFact::new(format!("resources of page {}", page + 1)));
            }
            Summary {
                meta: format!("{area} at {}, {}", num(a.x0), num(a.y0)),
                chips,
                chip: ChipFact::new("Form XObject"),
            }
        }
        VisualKind::ColorSpace(space) => {
            let count = swatch_count(space);
            let mut chips = vec![
                ChipFact::new(space.family.clone()),
                ChipFact::new(components(space.components)),
                ChipFact::new(count.clone()),
            ];
            if space.icc_profile {
                chips.push(ChipFact::new("ICC profile stream"));
            }
            Summary {
                meta: format!("{} · {count}", space.summary),
                chips,
                chip: ChipFact::new(format!("{} · {count}", space.family)),
            }
        }
        VisualKind::Shading(shading) => shading_summary(shading),
        VisualKind::Pattern(pattern) => Summary {
            meta: pattern.summary.clone(),
            chips: vec![ChipFact::new(pattern.summary.clone())],
            chip: ChipFact::new(pattern.summary.clone()),
        },
        VisualKind::Page(page) => {
            let page = format!("page {}", page + 1);
            Summary {
                meta: format!("{page}, as on the canvas"),
                chips: vec![ChipFact::new(page.clone())],
                chip: ChipFact::new(page),
            }
        }
    }
}

fn image_summary(image: &ImageVisual) -> Summary {
    let pixels = format!("{} × {} px", image.width, image.height);
    let bits = image
        .bits_per_component
        .map_or_else(|| "bpc from the data".to_owned(), |b| format!("{b} bpc"));
    let space = match (&image.color_space, image.image_mask) {
        (_, true) => "stencil mask".to_owned(),
        (Some(space), false) => space.clone(),
        (None, false) => "colour space from the data".to_owned(),
    };
    let alpha = if image.soft_mask { "SMask" } else { "no SMask" };
    Summary {
        meta: format!("{pixels} · {bits} · {space}"),
        chips: vec![ChipFact::new(space), ChipFact::new(bits), ChipFact::new(alpha)],
        chip: ChipFact::new(pixels),
    }
}

fn font_summary(font: &FontVisual) -> Summary {
    let kind = font.facts.subtype.as_deref().unwrap_or("font");
    let name = font.facts.base_font.as_deref().unwrap_or("unnamed");
    let codes = codes_shown(font.used);
    match font.program {
        FontProgram::Embedded => {
            let format = program_format(font);
            let subset = match &font.facts.subset_prefix {
                Some(tag) => format!("subset {tag}"),
                None => "not subset".to_owned(),
            };
            let bare = name.split_once('+').map_or(name, |(_, rest)| rest);
            Summary {
                meta: format!("embedded ({format}) · {codes}"),
                chips: vec![
                    ChipFact::new(format!("embedded · {format}")),
                    ChipFact::new(bare),
                    ChipFact::new(codes),
                    ChipFact::new(subset),
                ],
                chip: ChipFact::new("embedded"),
            }
        }
        FontProgram::Procedures => Summary {
            meta: format!("glyph procedures (Type 3) · {codes}"),
            chips: vec![
                ChipFact::new("Type 3 · glyph procedures"),
                ChipFact::new(name),
                ChipFact::new(codes),
            ],
            chip: ChipFact::new("glyph procedures"),
        },
        FontProgram::Missing => {
            let mut chips = vec![
                ChipFact::new(format!("{kind} · {name}")),
                ChipFact::warning("not embedded"),
            ];
            if is_standard_14(name) {
                chips.push(ChipFact::new("standard 14 font"));
            }
            Summary {
                meta: format!("{kind} · {name} · not embedded"),
                chips,
                chip: ChipFact::warning("not embedded"),
            }
        }
    }
}

fn shading_summary(shading: &ShadingVisual) -> Summary {
    let name = shading.shading_type.map_or("shading", shading_name);
    let stops = match shading.stops.len() {
        0 => None,
        1 => Some("1 stop".to_owned()),
        n => Some(format!("{n} stops")),
    };
    let mut meta = vec![name.to_owned()];
    meta.extend(stops.clone());
    meta.extend(shading.color_space.clone());
    let kind = shading
        .shading_type
        .map_or_else(|| "type unknown".to_owned(), |t| format!("Type {t} · {name}"));
    let mut chips = vec![ChipFact::new(kind)];
    if matches!(shading.shading_type, Some(2 | 3)) {
        let [start, end] = shading.extend;
        chips.push(ChipFact::new(format!("Extend {start} {end}")));
    }
    if let Some(t) = shading.function_type {
        chips.push(ChipFact::new(format!("Function {t} · {}", function_name(t))));
    }
    Summary {
        meta: meta.join(" · "),
        chips,
        chip: ChipFact::new(stops.map_or_else(|| name.to_owned(), |stops| format!("{name} · {stops}"))),
    }
}

/// What a preview of `visual` is made of, in the inspector's words: the rows under its card.
pub fn facts(visual: &Visual) -> Vec<Fact> {
    match &visual.kind {
        VisualKind::Image(image) => {
            let depth = match (image.image_mask, image.bits_per_component) {
                (true, _) => "1 bit, stencil mask".to_owned(),
                (false, Some(1)) => "1 bit per component".to_owned(),
                (false, Some(bits)) => format!("{bits} bits per component"),
                (false, None) => "set by the image data".to_owned(),
            };
            let space = match (&image.color_space, image.image_mask) {
                (_, true) => "none: paints the fill colour".to_owned(),
                (Some(space), false) => space.clone(),
                (None, false) => "set by the image data".to_owned(),
            };
            let alpha = if image.soft_mask {
                "soft mask (SMask)"
            } else {
                "none (no SMask)"
            };
            vec![
                Fact::new("Pixels", format!("{} × {}", image.width, image.height)),
                Fact::new("Depth", depth),
                Fact::new("Colour space", space),
                Fact::new("Alpha", alpha),
            ]
        }
        VisualKind::Font(font) => font_facts(font),
        VisualKind::Form(form) => {
            let a = &form.area;
            let mut facts = vec![Fact::new(
                "Area",
                format!(
                    "{} × {} pt at {}, {}",
                    num(a.width()),
                    num(a.height()),
                    num(a.x0),
                    num(a.y0)
                ),
            )];
            if let Some(page) = form.borrows_from {
                facts.push(Fact::new("Resources", format!("borrowed from page {}", page + 1)));
            }
            facts
        }
        VisualKind::ColorSpace(space) => {
            let shown = match space.family.as_str() {
                "Indexed" | "I" => format!("Indexed · {}", swatch_count(space)),
                "Separation" => "a tint ramp, paper to full ink".to_owned(),
                "DeviceN" => "each ink, full and half".to_owned(),
                _ if space.components == 1 => "a ramp, black to white".to_owned(),
                _ => format!("{} sample colours", space.swatches.len()),
            };
            let key = if is_indexed(space) { "Palette" } else { "Swatches" };
            vec![
                Fact::new("Space", space.summary.clone()),
                Fact::new("Components", space.components.to_string()),
                Fact::new(key, shown),
            ]
        }
        VisualKind::Shading(shading) => {
            let kind = shading.shading_type.map_or_else(
                || "type unknown".to_owned(),
                |t| format!("Type {t} ({})", shading_name(t)),
            );
            let mut facts = vec![Fact::new("Shading", kind)];
            if let Some(space) = &shading.color_space {
                facts.push(Fact::new("Colour space", space.clone()));
            }
            if let Some(t) = shading.function_type {
                facts.push(Fact::new("Function", format!("{} (type {t})", function_name(t))));
            }
            facts
        }
        VisualKind::Pattern(pattern) => vec![Fact::new("Pattern", pattern.summary.clone())],
        VisualKind::Page(page) => vec![Fact::new(
            "Picture",
            format!("page {}, as on the canvas", page + 1),
        )],
    }
}

fn font_facts(font: &FontVisual) -> Vec<Fact> {
    let program = match font.program {
        FontProgram::Embedded => format!("embedded ({})", program_format(font)),
        FontProgram::Procedures => "glyph procedures (Type 3)".to_owned(),
        FontProgram::Missing => "not embedded".to_owned(),
    };
    let mut facts = vec![Fact::new("Program", program).warn(font.program == FontProgram::Missing)];
    if font.program == FontProgram::Missing
        && let Some(name) = &font.facts.base_font
    {
        let value = if is_standard_14(name) {
            format!("{name} (standard 14)")
        } else {
            name.clone()
        };
        facts.push(Fact::new("Base font", value));
    }
    facts.push(Fact::new("Glyphs shown", codes_shown(font.used)));
    if let Some(encoding) = &font.facts.encoding {
        facts.push(Fact::new("Encoding", encoding.trim_start_matches('/')));
    }
    facts
}

/// `TrueType`, from the descriptor key that holds the program.
fn program_format(font: &FontVisual) -> &str {
    match font.facts.font_file.as_deref() {
        Some("FontFile") => "Type 1",
        Some("FontFile2") => "TrueType",
        Some("FontFile3") => "CFF / OpenType",
        _ => font.facts.subtype.as_deref().unwrap_or("font"),
    }
}

/// `2 distinct codes`.
fn codes_shown(used: usize) -> String {
    match used {
        0 => "none on the pages".to_owned(),
        1 => "1 distinct code".to_owned(),
        n => format!("{n} distinct codes"),
    }
}

fn components(n: usize) -> String {
    if n == 1 {
        "1 component".to_owned()
    } else {
        format!("{n} components")
    }
}

fn is_standard_14(name: &str) -> bool {
    STANDARD_14.contains(&name)
}

/// An `/Indexed` space: its swatches are its palette.
fn is_indexed(space: &ColorSpaceVisual) -> bool {
    matches!(space.family.as_str(), "Indexed" | "I")
}

/// `2 colours` for a palette, `9 swatches` otherwise.
fn swatch_count(space: &ColorSpaceVisual) -> String {
    let n = space.swatches.len();
    match (is_indexed(space), n) {
        (true, 1) => "1 colour".to_owned(),
        (true, _) => format!("{n} colours"),
        (false, 1) => "1 swatch".to_owned(),
        (false, _) => format!("{n} swatches"),
    }
}

/// The caption over the swatches of `space`: `INDEXED PALETTE · 8 COLOURS`.
pub(super) fn swatches_title(space: &ColorSpaceVisual) -> String {
    let what = if is_indexed(space) {
        "Indexed palette".to_owned()
    } else {
        space.family.clone()
    };
    format!("{what} · {}", swatch_count(space)).to_uppercase()
}

/// The text under swatch `i`: its palette index, or its components.
pub(super) fn swatch_value(space: &ColorSpaceVisual, i: usize) -> String {
    if is_indexed(space) {
        return i.to_string();
    }
    space.swatches.get(i).map_or_else(String::new, |values| {
        values.iter().map(|v| short(*v)).collect::<Vec<_>>().join(" ")
    })
}

/// The text under a gradient stop: `0 · .17 .42 .69`.
pub(super) fn stop_text(stop: &ShadingStop) -> String {
    let t = short(stop.t);
    if stop.color.is_empty() {
        return t;
    }
    let color: Vec<String> = stop.color.iter().map(|v| short(*v)).collect();
    format!("{t} · {}", color.join(" "))
}

/// A number the way a gradient caption writes it: whole numbers plain, others with two decimals
/// and no leading zero (`.17`).
fn short(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        return num(v.round());
    }
    let text = format!("{v:.2}");
    match text.strip_prefix("0.") {
        Some(rest) => format!(".{rest}"),
        None => text
            .strip_prefix("-0.")
            .map_or(text.clone(), |rest| format!("-.{rest}")),
    }
}

fn shading_name(t: i64) -> &'static str {
    match t {
        1 => "function-based",
        2 => "axial",
        3 => "radial",
        4 => "free-form mesh",
        5 => "lattice mesh",
        6 => "Coons patch mesh",
        7 => "tensor patch mesh",
        _ => "unknown",
    }
}

fn function_name(t: i64) -> &'static str {
    match t {
        0 => "sampled",
        2 => "exponential",
        3 => "stitching",
        4 => "PostScript",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_core::{ImageVisual, ObjRef, Visual, VisualKind};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn an_image_mask_has_no_colour_space_of_its_own() {
        let visual = Visual {
            obj: ObjRef::new(7, 0),
            kind: VisualKind::Image(ImageVisual {
                width: 8,
                height: 1,
                bits_per_component: Some(1),
                color_space: None,
                image_mask: true,
                soft_mask: false,
            }),
        };

        let shown: Vec<(&str, String)> = facts(&visual).into_iter().map(|f| (f.key, f.value)).collect();

        assert_eq!(
            shown,
            [
                ("Pixels", "8 × 1".to_owned()),
                ("Depth", "1 bit, stencil mask".to_owned()),
                ("Colour space", "none: paints the fill colour".to_owned()),
                ("Alpha", "none (no SMask)".to_owned()),
            ]
        );
        // Smoke: the header line says the same in short.
        assert_eq!(summary(&visual).meta, "8 × 1 px · 1 bpc · stencil mask");
    }

    #[test]
    fn gradient_numbers_read_like_the_design() {
        let stop = ShadingStop {
            t: 0.0,
            position: 0.0,
            color: vec![0.17, 0.42, 0.69],
        };

        assert_eq!(stop_text(&stop), "0 · .17 .42 .69");
        assert_eq!(short(0.8), ".80");
        assert_eq!(short(-0.25), "-.25");
        // Smoke: whole numbers stay whole, a stop without colour is its `t`.
        assert_eq!(
            stop_text(&ShadingStop {
                t: 1.0,
                position: 1.0,
                color: Vec::new()
            }),
            "1"
        );
    }
}
