//! One-line summaries and inspector rows for resource objects, per category.

use pdfxray_raw::{Dict, Object, RawDocument, Stream};

use super::format::{FLAG_NAMES, ProgramKey, opt, show, show_dict_entries, subset_prefix};
use crate::{NodeId, ObjRef, PropGroup, ResKind};

/// Reads dictionaries through indirect references.
pub(super) struct Lens<'a> {
    raw: &'a RawDocument,
}

/// A colour space, taken apart.
pub(super) struct ColorSpaceInfo {
    /// `DeviceRGB`, `ICCBased`, `Indexed`, ...
    pub(super) family: String,
    /// Number of colour components, when known.
    pub(super) components: Option<usize>,
    /// Family-specific parameters in display order.
    pub(super) params: Vec<(&'static str, String)>,
}

impl ColorSpaceInfo {
    /// `DeviceRGB`, or `ICCBased (N=3)`.
    pub(super) fn summary(&self) -> String {
        if self.params.is_empty() {
            return self.family.clone();
        }
        let params: Vec<String> = self.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{} ({})", self.family, params.join(", "))
    }
}

/// The embedded font program of a font.
pub(super) struct Program {
    pub(super) key: ProgramKey,
    pub(super) target: Option<ObjRef>,
    /// `/Subtype` of a `FontFile3` stream.
    pub(super) subtype: Option<String>,
}

impl<'a> Lens<'a> {
    pub(super) fn new(raw: &'a RawDocument) -> Self {
        Self { raw }
    }

    /// `value` with indirect references followed.
    pub(super) fn resolve(&self, value: &Object) -> Object {
        self.raw.resolve(value).into_owned()
    }

    /// The value of `key`, resolved; `None` when absent or null.
    pub(super) fn obj(&self, d: &Dict, key: &str) -> Option<Object> {
        let v = self.raw.resolve(&d.get(key.as_bytes())?.value).into_owned();
        (v != Object::Null).then_some(v)
    }

    pub(super) fn name(&self, d: &Dict, key: &str) -> Option<String> {
        self.obj(d, key)?
            .as_name()
            .map(pdfxray_raw::Name::to_string_lossy)
    }

    pub(super) fn int(&self, d: &Dict, key: &str) -> Option<i64> {
        self.obj(d, key)?.as_int()
    }

    pub(super) fn array(&self, d: &Dict, key: &str) -> Option<Vec<Object>> {
        let v = self.obj(d, key)?;
        Some(v.as_array()?.iter().map(|i| i.value.clone()).collect())
    }

    pub(super) fn dict(&self, d: &Dict, key: &str) -> Option<Dict> {
        self.obj(d, key)?.as_dict().cloned()
    }

    pub(super) fn target(d: &Dict, key: &str) -> Option<ObjRef> {
        d.get(key.as_bytes())?.value.as_ref_obj()
    }

    fn filters(&self, d: &Dict) -> Vec<String> {
        match self.obj(d, "Filter") {
            Some(Object::Name(n)) => vec![n.to_string_lossy()],
            Some(Object::Array(items)) => items
                .iter()
                .filter_map(|i| {
                    self.raw
                        .resolve(&i.value)
                        .as_name()
                        .map(pdfxray_raw::Name::to_string_lossy)
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    // ---- colour spaces ---------------------------------------------------------------------

    /// Takes a colour space apart; `depth` bounds the recursion into base / alternate spaces.
    pub(super) fn color_space(&self, o: &Object, depth: u8) -> ColorSpaceInfo {
        let o = self.raw.resolve(o).into_owned();
        let info = |family: &str, components| ColorSpaceInfo {
            family: family.to_owned(),
            components,
            params: Vec::new(),
        };
        if depth > 4 {
            return info("…", None);
        }
        let items: Vec<Object> = match &o {
            Object::Name(n) => {
                let family = n.to_string_lossy();
                let components = simple_components(&family);
                return info(&family, components);
            }
            Object::Array(a) => a.iter().map(|i| i.value.clone()).collect(),
            _ => return info("invalid", None),
        };
        let family = items
            .first()
            .map(|f| self.raw.resolve(f))
            .and_then(|f| f.as_name().map(pdfxray_raw::Name::to_string_lossy))
            .unwrap_or_else(|| "invalid".to_owned());
        let nested = |ix: usize| items.get(ix).map(|v| self.color_space(v, depth + 1));
        let mut out = info(&family, simple_components(&family));
        match family.as_str() {
            "ICCBased" => {
                let stream = items.get(1).map(|v| self.raw.resolve(v).into_owned());
                if let Some(d) = stream.as_ref().and_then(Object::as_dict) {
                    let n = self.int(d, "N");
                    out.components = n.and_then(|n| usize::try_from(n).ok());
                    out.params.extend(n.map(|n| ("N", n.to_string())));
                    if let Some(alt) = self.obj(d, "Alternate") {
                        out.params
                            .push(("Alternate", self.color_space(&alt, depth + 1).summary()));
                    }
                }
            }
            "Indexed" | "I" => {
                out.components = Some(1);
                out.params.extend(nested(1).map(|b| ("Base", b.summary())));
                let hival = items.get(2).map(|v| show(&self.raw.resolve(v)));
                out.params.extend(hival.map(|h| ("HiVal", h)));
            }
            "Separation" => {
                out.components = Some(1);
                let colorant = items.get(1).map(|v| show(&self.raw.resolve(v)));
                out.params.extend(colorant.map(|c| ("Colorant", c)));
                out.params.extend(nested(2).map(|a| ("Alternate", a.summary())));
            }
            "DeviceN" => {
                let names = items.get(1).map(|v| self.raw.resolve(v).into_owned());
                let names = names.as_ref().and_then(Object::as_array).unwrap_or_default();
                out.components = Some(names.len());
                let list: Vec<String> = names.iter().map(|n| show(&n.value)).collect();
                out.params.push(("Colorants", list.join(" ")));
                out.params.extend(nested(2).map(|a| ("Alternate", a.summary())));
            }
            "CalGray" | "CalRGB" | "Lab" => {
                let dict = items.get(1).map(|v| self.raw.resolve(v).into_owned());
                if let Some(d) = dict.as_ref().and_then(Object::as_dict) {
                    for (key, label) in [
                        ("WhitePoint", "WhitePoint"),
                        ("Gamma", "Gamma"),
                        ("Range", "Range"),
                    ] {
                        if let Some(v) = self.obj(d, key) {
                            out.params.push((label, show(&v)));
                        }
                    }
                }
            }
            "Pattern" => out.params.extend(nested(1).map(|u| ("Underlying", u.summary()))),
            _ => {}
        }
        out
    }

    // ---- summaries -------------------------------------------------------------------------

    /// One line describing `value` for the tree.
    pub(super) fn summary(&self, kind: ResKind, value: &Object) -> String {
        let resolved = self.raw.resolve(value).into_owned();
        match kind {
            ResKind::Font => resolved
                .as_dict()
                .map_or_else(|| "not a dictionary".to_owned(), |d| self.font_summary(d)),
            ResKind::XObject => match &resolved {
                Object::Stream(s) => self.xobject_summary(s),
                _ => "not a stream".to_owned(),
            },
            ResKind::ColorSpace => self.color_space(&resolved, 0).summary(),
            ResKind::Pattern => resolved
                .as_dict()
                .map_or_else(|| "not a dictionary".to_owned(), |d| self.pattern_summary(d)),
            ResKind::Shading => resolved
                .as_dict()
                .map_or_else(|| "not a dictionary".to_owned(), |d| self.shading_summary(d)),
            ResKind::ExtGState => resolved.as_dict().map_or_else(
                || "not a dictionary".to_owned(),
                |d| {
                    let keys: Vec<String> = d
                        .keys()
                        .filter(|k| **k != *"Type")
                        .map(pdfxray_raw::Name::to_string_lossy)
                        .collect();
                    if keys.is_empty() {
                        "empty".to_owned()
                    } else {
                        keys.join(" ")
                    }
                },
            ),
            ResKind::Properties => resolved
                .as_dict()
                .map_or_else(|| "not a dictionary".to_owned(), |d| self.properties_summary(d)),
        }
    }

    fn font_summary(&self, font: &Dict) -> String {
        let subtype = self.name(font, "Subtype").unwrap_or_else(|| "?".to_owned());
        let mut head = subtype.clone();
        if let Some(base) = self.name(font, "BaseFont") {
            head.push(' ');
            head.push_str(&base);
        }
        if subtype == "Type3" {
            let glyphs = self.dict(font, "CharProcs").map_or(0, |c| c.len());
            return format!("{head}, {glyphs} glyphs");
        }
        let embedded = if self.program(font).is_some() {
            "embedded"
        } else {
            "not embedded"
        };
        format!("{head}, {embedded}")
    }

    fn xobject_summary(&self, s: &Stream) -> String {
        let d = &s.dict;
        match self.name(d, "Subtype").as_deref() {
            Some("Image") => self.image_summary(d),
            Some("Form") => match self.obj(d, "BBox") {
                Some(b) => format!("Form, BBox {}", show(&b)),
                None => "Form, no BBox".to_owned(),
            },
            Some(other) => other.to_owned(),
            None => "XObject without /Subtype".to_owned(),
        }
    }

    fn image_summary(&self, d: &Dict) -> String {
        let mut parts = Vec::new();
        if let (Some(w), Some(h)) = (self.int(d, "Width"), self.int(d, "Height")) {
            parts.push(format!("{w}×{h}"));
        }
        let mask = self.obj(d, "ImageMask") == Some(Object::Bool(true));
        if mask {
            parts.push("image mask".to_owned());
        }
        if let Some(bpc) = self.int(d, "BitsPerComponent") {
            parts.push(format!("{bpc} bpc"));
        }
        if !mask && let Some(cs) = self.obj(d, "ColorSpace") {
            parts.push(self.color_space(&cs, 0).summary());
        }
        let filters = self.filters(d);
        if !filters.is_empty() {
            parts.push(filters.join("+"));
        }
        if parts.is_empty() {
            "Image".to_owned()
        } else {
            parts.join(", ")
        }
    }

    pub(super) fn pattern_summary(&self, d: &Dict) -> String {
        match self.int(d, "PatternType") {
            Some(1) => {
                let paint = match self.int(d, "PaintType") {
                    Some(1) => "coloured",
                    Some(2) => "uncoloured",
                    _ => "paint type unknown",
                };
                format!("Tiling, {paint}")
            }
            Some(2) => match self.dict(d, "Shading") {
                Some(s) => format!("Shading pattern, {}", self.shading_summary(&s)),
                None => "Shading pattern".to_owned(),
            },
            _ => "pattern type unknown".to_owned(),
        }
    }

    pub(super) fn shading_summary(&self, d: &Dict) -> String {
        let kind = self.int(d, "ShadingType").map_or_else(
            || "type unknown".to_owned(),
            |t| format!("{} (type {t})", shading_name(t)),
        );
        match self.obj(d, "ColorSpace") {
            Some(cs) => format!("{kind}, {}", self.color_space(&cs, 0).summary()),
            None => kind,
        }
    }

    fn properties_summary(&self, d: &Dict) -> String {
        let ty = self.name(d, "Type");
        if ty.as_deref() == Some("OCG")
            && let Some(Object::String(name)) = self.obj(d, "Name")
        {
            return format!("OCG {name}");
        }
        if let Some(ty) = ty {
            return format!("/Type /{ty}");
        }
        let keys: Vec<String> = d.keys().map(pdfxray_raw::Name::to_string_lossy).collect();
        if keys.is_empty() {
            "empty".to_owned()
        } else {
            keys.join(" ")
        }
    }

    // ---- fonts -----------------------------------------------------------------------------

    /// The `/FontDescriptor` of the font that carries glyph data: the font itself or, for a
    /// Type 0 font, its descendant.
    pub(super) fn descriptor(&self, font: &Dict) -> Option<(Option<ObjRef>, Dict)> {
        let carrier = self.descendant(font).map_or_else(|| font.clone(), |(_, d)| d);
        let dict = self.dict(&carrier, "FontDescriptor")?;
        Some((Self::target(&carrier, "FontDescriptor"), dict))
    }

    /// The first `/DescendantFonts` entry of a Type 0 font.
    pub(super) fn descendant(&self, font: &Dict) -> Option<(Option<ObjRef>, Dict)> {
        let list = self.obj(font, "DescendantFonts")?;
        let first = list.as_array()?.first()?;
        let dict = self.raw.resolve(&first.value).as_dict().cloned()?;
        Some((first.value.as_ref_obj(), dict))
    }

    /// Which font program stream the font embeds, if any.
    pub(super) fn program(&self, font: &Dict) -> Option<Program> {
        let (_, desc) = self.descriptor(font)?;
        ProgramKey::ALL.into_iter().find_map(|key| {
            let value = self.obj(&desc, key.key())?;
            let stream = value.as_stream()?;
            Some(Program {
                key,
                target: Self::target(&desc, key.key()),
                subtype: self.name(&stream.dict, "Subtype"),
            })
        })
    }

    fn font_groups(&self, font: &Dict) -> Vec<PropGroup> {
        let subtype = self.name(font, "Subtype");
        let base = self.name(font, "BaseFont");
        let type3 = subtype.as_deref() == Some("Type3");
        let type0 = subtype.as_deref() == Some("Type0");
        let mut warnings = Vec::new();
        let mut g =
            PropGroup::new("Font").row("Subtype", subtype.clone().unwrap_or_else(|| "missing".to_owned()));
        g = opt(g, "BaseFont", base.clone());
        g = self.encoding_row(g, font);

        // Font program.
        if type3 {
            g = g.row("Embedded", "n/a (Type 3 glyph procedures)");
        } else {
            if let Some(p) = self.program(font) {
                g = g.row("Embedded", "yes");
                let label = match &p.subtype {
                    Some(st) => format!("{} /{st}", p.key.key()),
                    None => p.key.key().to_owned(),
                };
                g = if let Some(t) = p.target {
                    g.link("Font program", format!("{label} ({t})"), NodeId::Object(t))
                } else {
                    g.row("Font program", label)
                };
            } else {
                g = g.row("Embedded", "no");
                warnings.push("font program is not embedded".to_owned());
            }
            let subset = base.as_deref().and_then(subset_prefix);
            g = g.row(
                "Subset",
                subset.map_or_else(|| "no".to_owned(), ToOwned::to_owned),
            );
        }

        // ToUnicode.
        match font.get(b"ToUnicode") {
            Some(v) if self.raw.resolve(&v.value).as_stream().is_some() => {
                g = match v.value.as_ref_obj() {
                    Some(r) => g.link("ToUnicode", format!("yes ({r})"), NodeId::Object(r)),
                    None => g.row("ToUnicode", "yes"),
                };
            }
            _ => {
                g = g.row("ToUnicode", "no");
                warnings.push("no /ToUnicode map: text extraction depends on the encoding alone".to_owned());
            }
        }

        // Widths.
        if !type0 {
            let widths = self.array(font, "Widths");
            let (first, last) = (self.int(font, "FirstChar"), self.int(font, "LastChar"));
            if let Some(w) = &widths {
                let mut text = format!("{} entries", w.len());
                if let (Some(f), Some(l)) = (first, last) {
                    text = format!("{text}, FirstChar {f}, LastChar {l}");
                    let expected = l.checked_sub(f).and_then(|d| usize::try_from(d + 1).ok());
                    if expected.is_some_and(|e| e != w.len()) {
                        warnings.push(format!(
                            "/Widths has {} entries but FirstChar {f}..LastChar {l} spans {}",
                            w.len(),
                            l - f + 1
                        ));
                    }
                }
                g = g.row("Widths", text);
            } else if first.is_some() || last.is_some() {
                g = opt(g, "FirstChar", first.map(|v| v.to_string()));
                g = opt(g, "LastChar", last.map(|v| v.to_string()));
            }
        }

        if type3 {
            g = opt(g, "FontMatrix", self.obj(font, "FontMatrix").map(|v| show(&v)));
            g = opt(g, "FontBBox", self.obj(font, "FontBBox").map(|v| show(&v)));
            g = opt(
                g,
                "CharProcs",
                self.dict(font, "CharProcs")
                    .map(|c| format!("{} glyph procedures", c.len())),
            );
            if let Some(res) = self.dict(font, "Resources") {
                g = g.row("Resources", format!("own ({} categories)", res.len()));
            }
        }
        for w in warnings {
            g = g.row("Warning", w);
        }

        let mut groups = vec![g];
        if type0 {
            groups.extend(self.descendant_group(font));
        }
        groups.extend(self.descriptor_group(font));
        groups
    }

    fn encoding_row(&self, g: PropGroup, font: &Dict) -> PropGroup {
        let Some(value) = self.obj(font, "Encoding") else {
            return g;
        };
        let text = match &value {
            Object::Name(n) => n.to_string(),
            Object::Dict(d) => {
                let base = self
                    .name(d, "BaseEncoding")
                    .map_or_else(String::new, |b| format!("BaseEncoding /{b}, "));
                let diffs = self
                    .array(d, "Differences")
                    .map_or(0, |a| a.iter().filter(|o| matches!(o, Object::Name(_))).count());
                format!("dictionary ({base}{diffs} differences)")
            }
            Object::Stream(_) => "CMap stream".to_owned(),
            other => show(other),
        };
        match Self::target(font, "Encoding") {
            Some(r) => g.link("Encoding", text, NodeId::Object(r)),
            None => g.row("Encoding", text),
        }
    }

    fn descendant_group(&self, font: &Dict) -> Option<PropGroup> {
        let (target, d) = self.descendant(font)?;
        let mut g = PropGroup::new("Descendant font");
        g = match target {
            Some(t) => g.link("Object", t.to_string(), NodeId::Object(t)),
            None => g.row("Object", "direct"),
        };
        g = opt(g, "Subtype", self.name(&d, "Subtype"));
        g = opt(g, "BaseFont", self.name(&d, "BaseFont"));
        if let Some(info) = self.dict(&d, "CIDSystemInfo") {
            let registry = self.obj(&info, "Registry").map(|v| string_text(&v));
            let ordering = self.obj(&info, "Ordering").map(|v| string_text(&v));
            let supplement = self.int(&info, "Supplement");
            g = g.row(
                "CIDSystemInfo",
                format!(
                    "{}-{}-{}",
                    registry.unwrap_or_default(),
                    ordering.unwrap_or_default(),
                    supplement.unwrap_or(0)
                ),
            );
        }
        g = opt(
            g,
            "CIDToGIDMap",
            self.obj(&d, "CIDToGIDMap").map(|v| match v {
                Object::Name(n) => n.to_string(),
                _ => "stream".to_owned(),
            }),
        );
        g = opt(g, "DW", self.obj(&d, "DW").map(|v| show(&v)));
        Some(g)
    }

    fn descriptor_group(&self, font: &Dict) -> Option<PropGroup> {
        let (target, d) = self.descriptor(font)?;
        let mut g = PropGroup::new("Font descriptor");
        g = match target {
            Some(t) => g.link("Object", t.to_string(), NodeId::Object(t)),
            None => g.row("Object", "direct"),
        };
        g = opt(g, "FontName", self.name(&d, "FontName"));
        if let Some(flags) = self.int(&d, "Flags") {
            g = g.row("Flags", describe_flags(flags));
        }
        for key in [
            "Ascent",
            "Descent",
            "CapHeight",
            "XHeight",
            "FontBBox",
            "ItalicAngle",
            "StemV",
        ] {
            g = opt(g, key, self.obj(&d, key).map(|v| show(&v)));
        }
        Some(g)
    }

    // ---- other categories ------------------------------------------------------------------

    fn xobject_groups(&self, s: &Stream) -> Vec<PropGroup> {
        let d = &s.dict;
        match self.name(d, "Subtype").as_deref() {
            Some("Image") => vec![self.image_group(s)],
            Some("Form") => vec![self.form_group(s)],
            other => vec![PropGroup::new("XObject").row("Subtype", other.unwrap_or("missing"))],
        }
    }

    fn image_group(&self, s: &Stream) -> PropGroup {
        let d = &s.dict;
        let mut g = PropGroup::new("Image");
        g = opt(g, "Width", self.int(d, "Width").map(|v| v.to_string()));
        g = opt(g, "Height", self.int(d, "Height").map(|v| v.to_string()));
        g = opt(
            g,
            "BitsPerComponent",
            self.int(d, "BitsPerComponent").map(|v| v.to_string()),
        );
        if let Some(cs) = self.obj(d, "ColorSpace") {
            let info = self.color_space(&cs, 0);
            g = g.row("ColorSpace", info.summary());
            g = opt(g, "Components", info.components.map(|c| c.to_string()));
        }
        let filters = self.filters(d);
        if !filters.is_empty() {
            g = g.row("Filter", filters.join(", "));
        }
        let mask = self.obj(d, "ImageMask") == Some(Object::Bool(true));
        g = g.row("ImageMask", if mask { "yes" } else { "no" });
        g = match Self::target(d, "SMask") {
            Some(r) => g.link("SMask", format!("yes ({r})"), NodeId::Object(r)),
            None => g.row("SMask", if d.get(b"SMask").is_some() { "yes" } else { "no" }),
        };
        if d.get(b"Mask").is_some() {
            g = g.row("Mask", "yes");
        }
        g = opt(g, "Interpolate", self.obj(d, "Interpolate").map(|v| show(&v)));
        g = opt(g, "Decode", self.obj(d, "Decode").map(|v| show(&v)));
        g = g.row("Encoded size", format!("{} bytes", s.data.len()));
        match self.raw.decode_stream(s) {
            Ok(dec) => {
                let note = if dec.complete {
                    ""
                } else {
                    " (incomplete: not all filters could be applied)"
                };
                g.row("Decoded size", format!("{} bytes{note}", dec.bytes.len()))
            }
            Err(e) => g.row("Decoded size", format!("unavailable: {e}")),
        }
    }

    fn form_group(&self, s: &Stream) -> PropGroup {
        let d = &s.dict;
        let mut g = PropGroup::new("Form");
        g = opt(g, "BBox", self.obj(d, "BBox").map(|v| show(&v)));
        g = opt(g, "Matrix", self.obj(d, "Matrix").map(|v| show(&v)));
        if let Some(group) = self.dict(d, "Group") {
            g = g.row("Group", show_dict_entries(&group));
        }
        if let Some(res) = self.dict(d, "Resources") {
            let cats: Vec<String> = res.keys().map(pdfxray_raw::Name::to_string_lossy).collect();
            g = g.row("Resources", format!("own ({})", cats.join(", ")));
        } else {
            g = g.row("Resources", "none: uses the resources of whatever paints it");
        }
        if let Some(r) = Self::target(d, "OC") {
            g = g.link("OC", r.to_string(), NodeId::Object(r));
        }
        g.row("Encoded size", format!("{} bytes", s.data.len()))
    }

    fn color_space_group(&self, value: &Object) -> PropGroup {
        let info = self.color_space(value, 0);
        let mut g = PropGroup::new("Color space").row("Family", info.family.clone());
        g = opt(g, "Components", info.components.map(|c| c.to_string()));
        for (k, v) in &info.params {
            g = g.row(*k, v.clone());
        }
        g
    }

    fn pattern_group(&self, d: &Dict) -> PropGroup {
        let mut g = PropGroup::new("Pattern");
        let kind = match self.int(d, "PatternType") {
            Some(1) => "1 (tiling)".to_owned(),
            Some(2) => "2 (shading)".to_owned(),
            Some(n) => n.to_string(),
            None => "missing".to_owned(),
        };
        g = g.row("PatternType", kind);
        if let Some(p) = self.int(d, "PaintType") {
            g = g.row(
                "PaintType",
                if p == 1 {
                    "1 (coloured)".to_owned()
                } else if p == 2 {
                    "2 (uncoloured)".to_owned()
                } else {
                    p.to_string()
                },
            );
        }
        if let Some(t) = self.int(d, "TilingType") {
            g = g.row("TilingType", t.to_string());
        }
        for key in ["BBox", "XStep", "YStep", "Matrix"] {
            g = opt(g, key, self.obj(d, key).map(|v| show(&v)));
        }
        if let Some(res) = self.dict(d, "Resources") {
            let cats: Vec<String> = res.keys().map(pdfxray_raw::Name::to_string_lossy).collect();
            g = g.row("Resources", format!("own ({})", cats.join(", ")));
        }
        if let Some(sh) = self.dict(d, "Shading") {
            let text = self.shading_summary(&sh);
            g = match Self::target(d, "Shading") {
                Some(r) => g.link("Shading", format!("{text} ({r})"), NodeId::Object(r)),
                None => g.row("Shading", text),
            };
        }
        if let Some(r) = Self::target(d, "ExtGState") {
            g = g.link("ExtGState", r.to_string(), NodeId::Object(r));
        }
        g
    }

    fn shading_group(&self, d: &Dict) -> PropGroup {
        let mut g = PropGroup::new("Shading");
        g = opt(
            g,
            "ShadingType",
            self.int(d, "ShadingType")
                .map(|t| format!("{t} ({})", shading_name(t))),
        );
        if let Some(cs) = self.obj(d, "ColorSpace") {
            g = g.row("ColorSpace", self.color_space(&cs, 0).summary());
        }
        for key in ["Coords", "Domain", "Extend", "BBox", "Background", "Matrix"] {
            g = opt(g, key, self.obj(d, key).map(|v| show(&v)));
        }
        match self.obj(d, "Function") {
            Some(Object::Array(a)) => g = g.row("Function", format!("array of {} functions", a.len())),
            Some(f) => {
                let ty = f.as_dict().and_then(|fd| self.int(fd, "FunctionType"));
                let text = ty.map_or_else(|| "present".to_owned(), |t| format!("type {t}"));
                g = match Self::target(d, "Function") {
                    Some(r) => g.link("Function", format!("{text} ({r})"), NodeId::Object(r)),
                    None => g.row("Function", text),
                };
            }
            None => {}
        }
        g
    }

    fn dictionary_group(title: &str, d: &Dict) -> PropGroup {
        let mut g = PropGroup::new(title);
        for (k, v) in &d.entries {
            let key = k.value.to_string();
            g = match v.value.as_ref_obj() {
                Some(r) => g.link(key, r.to_string(), NodeId::Object(r)),
                None => g.row(key, show(&v.value)),
            };
        }
        g
    }

    /// Category-specific inspector groups for a resource whose (resolved) value is `value`.
    pub(super) fn details(&self, kind: ResKind, value: &Object) -> Vec<PropGroup> {
        let resolved = self.raw.resolve(value).into_owned();
        match (kind, &resolved) {
            (ResKind::Font, Object::Dict(d)) => self.font_groups(d),
            (ResKind::XObject, Object::Stream(s)) => self.xobject_groups(s),
            (ResKind::ColorSpace, v) => vec![self.color_space_group(v)],
            (ResKind::Pattern, Object::Dict(d) | Object::Stream(Stream { dict: d, .. })) => {
                vec![self.pattern_group(d)]
            }
            (ResKind::Shading, Object::Dict(d) | Object::Stream(Stream { dict: d, .. })) => {
                vec![self.shading_group(d)]
            }
            (ResKind::ExtGState, Object::Dict(d)) => vec![Self::dictionary_group("ExtGState", d)],
            (ResKind::Properties, Object::Dict(d)) => vec![Self::dictionary_group("Properties", d)],
            _ => vec![PropGroup::new(kind.key()).row("Value", show(&resolved))],
        }
    }
}

fn simple_components(family: &str) -> Option<usize> {
    match family {
        "DeviceGray" | "G" | "CalGray" => Some(1),
        "DeviceRGB" | "RGB" | "CalRGB" | "Lab" => Some(3),
        "DeviceCMYK" | "CMYK" => Some(4),
        _ => None,
    }
}

fn shading_name(t: i64) -> &'static str {
    match t {
        1 => "function-based",
        2 => "axial",
        3 => "radial",
        4 => "free-form triangle mesh",
        5 => "lattice-form triangle mesh",
        6 => "Coons patch mesh",
        7 => "tensor-product patch mesh",
        _ => "unknown",
    }
}

fn string_text(o: &Object) -> String {
    match o {
        Object::String(s) => String::from_utf8_lossy(&s.bytes).into_owned(),
        other => other.to_string(),
    }
}

/// `32 (Nonsymbolic)` for a `/Flags` value.
fn describe_flags(flags: i64) -> String {
    let names: Vec<&str> = FLAG_NAMES
        .iter()
        .filter(|(bit, _)| flags.checked_shr(u32::from(*bit - 1)).unwrap_or(0) & 1 == 1)
        .map(|(_, n)| *n)
        .collect();
    if names.is_empty() {
        flags.to_string()
    } else {
        format!("{flags} ({})", names.join(", "))
    }
}
