//! Inspector properties of operators, glyphs, marked-content sequences and artifacts.

use std::collections::BTreeSet;

use super::tree::{artifact_suffix, seq_summary, truncate};
use super::{PageData, stream_form};
use crate::capture::{
    CapturedPage, ColorInfo, GState, MarkedSeq, OpRecord, PaintItem, PaintKind, StreamRef, TextState,
};
use crate::numeric::usize_from_u32;
use crate::{Document, NodeId, ObjRef, PageIx, PropGroup, Props, ResKind, fmt_rect, num};

/// Glyph links listed on a text operator.
const MAX_GLYPH_LINKS: usize = 50;
/// Longest extracted text shown for a marked-content sequence.
const MAX_TEXT_CHARS: usize = 500;
/// Longest text shown for a text operator.
const MAX_OP_TEXT_CHARS: usize = 200;

impl Document {
    /// Props of an operator node.
    pub(super) fn op_props(&self, page: PageIx, ix: u32) -> Option<Props> {
        let data = self.content.data(page)?;
        let op = data.op(ix)?;
        let gs = &*op.gstate;
        let mut groups = vec![operator_group(data, op)];
        if let Some(form) = form_group(data, op) {
            groups.push(form);
        }
        groups.push(geometry_group(data, op));
        groups.push(gstate_group(self.resources_owner(page, &data.cap, op), gs));
        if is_text_operator(&op.operator) {
            groups.push(self.text_group(page, data, op, gs));
        }
        if let Some(group) = marked_group(page, data, op) {
            groups.push(group);
        }
        if let Some(group) = glyphs_group(page, data, op) {
            groups.push(group);
        }
        Some(Props {
            id: NodeId::Op { page, ix },
            title: op.operator.clone(),
            kind: "operator",
            groups,
        })
    }

    /// Props of a glyph node.
    pub(super) fn glyph_props(&self, page: PageIx, op_ix: u32, glyph: u32) -> Option<Props> {
        let data = self.content.data(page)?;
        let op = data.op(op_ix)?;
        let item = data.glyph_item(op_ix, glyph)?;
        let PaintKind::Glyph {
            glyph_index,
            char_code,
            unicode,
            origin,
            advance,
        } = &item.kind
        else {
            return None;
        };
        let shown = unicode.as_deref().unwrap_or_default();
        let code_points: Vec<String> = shown.chars().map(|c| format!("U+{:04X}", u32::from(c))).collect();
        let mut main = PropGroup::new("Glyph")
            .row("Unicode", if shown.is_empty() { "(none)" } else { shown })
            .row("Code point", code_points.join(" "))
            .row(
                "Char code",
                if *char_code == 0 {
                    "unknown".to_owned()
                } else {
                    format!("{char_code} (0x{char_code:X})")
                },
            )
            .row("Glyph index", glyph_index.to_string())
            .row("Origin", format!("({}, {})", num(origin.x), num(origin.y)))
            .row("Advance", num(*advance))
            .row("Bounding box", fmt_rect(item.bbox));
        if shown.is_empty() {
            main.rows.retain(|r| r.key != "Code point");
        }
        let ts = &item.gstate.text;
        let mut text = PropGroup::new("Text state");
        text = self.font_rows(text, page, &data.cap, op, ts);
        text = text
            .row("Font size", num(ts.font_size))
            .row("Effective size", num(effective_size(ts, &item.gstate.ctm)))
            .row("Render mode", render_mode(ts.render_mode));
        let mut context = PropGroup::new("Context").link(
            "Operator",
            format!("{} (op {op_ix})", op.operator),
            NodeId::Op { page, ix: op_ix },
        );
        for &s in &data.enclosing(op) {
            if let Some(seq) = data.seq(s) {
                context = marked_row(context, page, data, seq);
            }
        }
        Some(Props {
            id: NodeId::Glyph {
                page,
                op: op_ix,
                glyph,
            },
            title: if shown.is_empty() {
                format!("Glyph {glyph_index}")
            } else {
                format!("Glyph \"{shown}\"")
            },
            kind: "glyph",
            groups: vec![main, text, context],
        })
    }

    /// Props of a marked-content node (all sequences with the MCID).
    pub(super) fn mc_props(&self, page: PageIx, mcid: i32) -> Option<Props> {
        let data = self.content.data(page)?;
        let seqs: Vec<&MarkedSeq> = data.mc_seqs(mcid, |_| true).collect();
        let first = *seqs.first()?;
        let glyphs: BTreeSet<u32> = seqs.iter().flat_map(|s| data.glyph_ids(s)).collect();
        let text = data.seqs_text(seqs.iter().copied());
        let streams: BTreeSet<Option<ObjRef>> = seqs.iter().map(|s| stream_form(&s.stream)).collect();

        let mut main = PropGroup::new("Marked content")
            .row("Tag", first.tag.clone())
            .row("MCID", mcid.to_string());
        if seqs.len() > 1 {
            main = main.row("Occurrences", seqs.len().to_string());
        }
        if streams.len() > 1 {
            main = main.row("Content streams", streams.len().to_string());
        }
        if let Some(name) = &first.property_resource {
            main = main.row("Property list", format!("/{name}"));
        }
        for (key, value) in [
            ("ActualText", &first.actual_text),
            ("Alt", &first.alt),
            ("Lang", &first.lang),
        ] {
            if let Some(value) = value {
                main = main.row(key, value.clone());
            }
        }
        main = sequence_rows(main, page, first);
        if let Some(bbox) = PageData::seqs_bbox(seqs.iter().copied()) {
            main = main.row("Bounding box", fmt_rect(bbox));
        }
        main = main
            .row("Glyphs", glyphs.len().to_string())
            .row("Text", truncate(&text, MAX_TEXT_CHARS));
        let mut groups = vec![main, properties_group(first)];
        if seqs.len() > 1 {
            let mut occurrences = PropGroup::new("Occurrences");
            for (n, seq) in seqs.iter().enumerate() {
                occurrences = occurrences.link(
                    format!("#{}", n + 1),
                    format!("{} (op {})", seq.tag, seq.begin_op),
                    NodeId::Op {
                        page,
                        ix: seq.begin_op,
                    },
                );
            }
            groups.push(occurrences);
        }
        if let Some(enclosing) = parents_group(page, data, first) {
            groups.push(enclosing);
        }
        Some(Props {
            id: NodeId::MarkedContent { page, mcid },
            title: format!("{} MCID {mcid}", first.tag),
            kind: "marked-content",
            groups,
        })
    }

    /// Props of an artifact node.
    pub(super) fn artifact_props(&self, page: PageIx, number: u32) -> Option<Props> {
        let data = self.content.data(page)?;
        let seq = data.artifact_seq(number)?;
        let info = seq.artifact.as_ref()?;
        let glyphs = data.glyph_ids(seq);
        let text = data.seqs_text(std::iter::once(seq));

        let mut main = PropGroup::new("Artifact")
            .row("Tag", seq.tag.clone())
            .row("Sequence", number.to_string());
        if let Some(kind) = &info.kind {
            main = main.row("Type", format!("/{kind}"));
        }
        if let Some(subtype) = &info.subtype {
            main = main.row("Subtype", format!("/{subtype}"));
        }
        if let Some(bbox) = info.bbox {
            main = main.row("BBox", fmt_rect(bbox));
        }
        if !info.attached.is_empty() {
            main = main.row("Attached", info.attached.join(" "));
        }
        if let Some(mcid) = seq.mcid {
            main = main.row("MCID", mcid.to_string());
        }
        main = sequence_rows(main, page, seq);
        if let Some(bbox) = seq.bbox {
            main = main.row("Bounding box", fmt_rect(bbox));
        }
        main = main
            .row("Glyphs", glyphs.len().to_string())
            .row("Text", truncate(&text, MAX_TEXT_CHARS));
        let mut groups = vec![main, properties_group(seq)];
        if let Some(enclosing) = parents_group(page, data, seq) {
            groups.push(enclosing);
        }
        Some(Props {
            id: NodeId::Artifact { page, seq: number },
            title: format!("Artifact{}", artifact_suffix(info)),
            kind: "artifact",
            groups,
        })
    }

    /// The object whose `/Resources` the operator resolved names against: the innermost Form
    /// `XObject` it runs in, else the page's.
    pub(super) fn resources_owner(&self, page: PageIx, cap: &CapturedPage, op: &OpRecord) -> Option<ObjRef> {
        let form = std::iter::successors(op.xobject, |&s| cap.xobjects.get(usize_from_u32(s))?.parent)
            .take(cap.xobjects.len())
            .find_map(|s| cap.xobjects.get(usize_from_u32(s))?.obj);
        form.or_else(|| self.page(page)?.resources_from)
    }

    /// `Font` and `Font object` rows.
    fn font_rows(
        &self,
        group: PropGroup,
        page: PageIx,
        cap: &CapturedPage,
        op: &OpRecord,
        ts: &TextState,
    ) -> PropGroup {
        let mut group = group;
        match (&ts.font, self.resources_owner(page, cap, op)) {
            (Some(name), Some(owner)) => {
                group = group.link(
                    "Font",
                    name.clone(),
                    NodeId::Resource {
                        owner,
                        kind: ResKind::Font,
                        name: name.clone(),
                    },
                );
            }
            (Some(name), None) => group = group.row("Font", name.clone()),
            (None, _) => group = group.row("Font", "(none)"),
        }
        if let Some(font) = ts.font_ref {
            group = group.link("Font object", font.to_string(), NodeId::Object(font));
        }
        group
    }

    fn text_group(&self, page: PageIx, data: &PageData, op: &OpRecord, gs: &GState) -> PropGroup {
        let ts = &gs.text;
        let mut group = PropGroup::new("Text state");
        group = self.font_rows(group, page, &data.cap, op, ts);
        group = group
            .row("Font size", num(ts.font_size))
            .row("Effective size", num(effective_size(ts, &gs.ctm)))
            .row("Tc", num(ts.char_spacing))
            .row("Tw", num(ts.word_spacing))
            .row("Tz", num(ts.horizontal_scaling))
            .row("TL", num(ts.leading))
            .row("Ts", num(ts.rise))
            .row("Tr", render_mode(ts.render_mode));
        if is_showing_operator(&op.operator) {
            let text: String = data
                .op_glyphs(op)
                .filter_map(|i| super::glyph_unicode(i))
                .collect();
            group = group.row("Text", truncate(&text, MAX_OP_TEXT_CHARS));
        }
        group
    }
}

/// Operators that show text.
fn is_showing_operator(operator: &str) -> bool {
    matches!(operator, "Tj" | "TJ" | "'" | "\"")
}

/// Operators that show text or change the text state.
fn is_text_operator(operator: &str) -> bool {
    is_showing_operator(operator)
        || matches!(
            operator,
            "Tf" | "Tc" | "Tw" | "Tz" | "TL" | "Ts" | "Tr" | "Tm" | "Td" | "TD" | "T*" | "BT"
        )
}

fn operator_group(data: &PageData, op: &OpRecord) -> PropGroup {
    let mut group = PropGroup::new("Operator")
        .row("Operator", op.operator.clone())
        .row("Operands", op.operands.join(" "))
        .row("Execution index", op.ix.to_string());
    group = match &op.stream {
        StreamRef::PageContent { index } => group.row("Stream", format!("Page content #{index}")),
        StreamRef::FormXObject {
            obj: Some(obj), name, ..
        } => group.link("Stream", format!("Form /{name} ({obj})"), NodeId::Object(*obj)),
        StreamRef::FormXObject { obj: None, name, .. } => group.row("Stream", format!("Form /{name}")),
    };
    group = group.row("Local index", op.local_ix.to_string());
    if let Some((start, end)) = op.span {
        group = group.row("Byte span", format!("{start}..{end}"));
    }
    for note in data.cap.notes.iter().filter(|n| n.op == Some(op.ix)) {
        group = group.row("Note", note.message.clone());
    }
    group
}

/// Facts about the Form `XObject` a `Do` drew.
fn form_group(data: &PageData, op: &OpRecord) -> Option<PropGroup> {
    let scope = data
        .cap
        .xobjects
        .get(usize_from_u32(*data.scope_of.get(&op.ix)?))?;
    let mut group = PropGroup::new("Form XObject").row("Name", format!("/{}", scope.name));
    if let Some(obj) = scope.obj {
        group = group.link("Object", obj.to_string(), NodeId::Object(obj));
    }
    group = group.row("Matrix", fmt_matrix(&scope.matrix));
    if let Some([x0, y0, x1, y1]) = scope.bbox {
        group = group.row(
            "BBox",
            format!("[{} {} {} {}]", num(x0), num(y0), num(x1), num(y1)),
        );
    }
    if let Some(bbox) = scope.page_bbox {
        group = group.row("Page-space box", fmt_rect(bbox));
    }
    group = group.row("CTM inside", fmt_matrix(&scope.ctm));
    if let (Some(first), Some(last)) = (scope.first_op, scope.last_op) {
        group = group.row("Operators", format!("{first}..{last}"));
    }
    Some(group)
}

fn geometry_group(data: &PageData, op: &OpRecord) -> PropGroup {
    let items = data.op_items(op);
    let mut group = PropGroup::new("Geometry");
    if let Some(bbox) = data.op_bbox(op) {
        group = group.row("Bounding box", fmt_rect(bbox));
    }
    group = group.row("Painted items", items.len().to_string());
    if !items.is_empty() {
        group = group.row("Paints", paint_summary(items));
    }
    group
}

/// `Glyph x5`, `Image 2 x 2 px`, kinds in order of first appearance.
fn paint_summary(items: &[PaintItem]) -> String {
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for label in items.iter().map(|i| kind_label(&i.kind)) {
        match kinds.iter_mut().find(|(l, _)| *l == label) {
            Some((_, n)) => *n += 1,
            None => kinds.push((label, 1)),
        }
    }
    kinds
        .into_iter()
        .map(|(label, n)| {
            if n > 1 {
                format!("{label} \u{d7}{n}")
            } else {
                label
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn kind_label(kind: &PaintKind) -> String {
    match kind {
        PaintKind::Fill => "Fill".to_owned(),
        PaintKind::Stroke => "Stroke".to_owned(),
        PaintKind::FillStroke => "Fill+Stroke".to_owned(),
        PaintKind::Clip => "Clip".to_owned(),
        PaintKind::Glyph { .. } => "Glyph".to_owned(),
        PaintKind::Image {
            width,
            height,
            inline,
        } => format!(
            "{}Image {width} \u{d7} {height} px",
            if *inline { "Inline " } else { "" }
        ),
        PaintKind::Shading => "Shading".to_owned(),
    }
}

fn gstate_group(owner: Option<ObjRef>, gs: &GState) -> PropGroup {
    let mut group = PropGroup::new("Graphics state")
        .row("CTM", fmt_matrix(&gs.ctm))
        .row("Fill color", color_value(&gs.fill))
        .row("Fill space", color_space(&gs.fill))
        .row("Stroke color", color_value(&gs.stroke))
        .row("Stroke space", color_space(&gs.stroke))
        .row("Line width", num(gs.line_width))
        .row("Line cap", format!("{} ({})", gs.line_cap, cap_name(gs.line_cap)))
        .row(
            "Line join",
            format!("{} ({})", gs.line_join, join_name(gs.line_join)),
        )
        .row("Dash", dash_value(&gs.dash_array, gs.dash_phase))
        .row("Alpha fill", num(gs.alpha_fill))
        .row("Alpha stroke", num(gs.alpha_stroke))
        .row("Blend mode", gs.blend_mode.clone())
        .row("Soft mask", if gs.soft_mask { "active" } else { "none" });
    group = match (&gs.ext_gstate, owner) {
        (Some(name), Some(owner)) => group.link(
            "ExtGState",
            format!("/{name}"),
            NodeId::Resource {
                owner,
                kind: ResKind::ExtGState,
                name: name.clone(),
            },
        ),
        (Some(name), None) => group.row("ExtGState", format!("/{name}")),
        (None, _) => group.row("ExtGState", "none"),
    };
    group.row("Clip box", gs.clip.map_or_else(|| "none".to_owned(), fmt_rect))
}

fn color_value(color: &ColorInfo) -> String {
    let mut parts: Vec<String> = color.components.iter().map(|c| num(*c)).collect();
    if let Some(pattern) = &color.pattern {
        parts.push(format!("/{pattern}"));
    }
    parts.join(" ")
}

fn color_space(color: &ColorInfo) -> String {
    match &color.resource {
        Some(name) => format!("{} (/{name})", color.space),
        None => color.space.clone(),
    }
}

fn dash_value(array: &[f64], phase: f64) -> String {
    if array.is_empty() {
        return "solid".to_owned();
    }
    let items: Vec<String> = array.iter().map(|d| num(*d)).collect();
    format!("[{}] {}", items.join(" "), num(phase))
}

fn cap_name(cap: u8) -> &'static str {
    match cap {
        0 => "butt",
        1 => "round",
        2 => "projecting square",
        _ => "invalid",
    }
}

fn join_name(join: u8) -> &'static str {
    match join {
        0 => "miter",
        1 => "round",
        2 => "bevel",
        _ => "invalid",
    }
}

/// `3 (invisible)`.
fn render_mode(mode: u8) -> String {
    let meaning = match mode {
        0 => "fill",
        1 => "stroke",
        2 => "fill, then stroke",
        3 => "invisible",
        4 => "fill and clip",
        5 => "stroke and clip",
        6 => "fill, stroke and clip",
        7 => "clip",
        _ => "invalid",
    };
    format!("{mode} ({meaning})")
}

/// `Tfs x ||(0, 1) . (Tm x CTM)||`: the font size as it lands on the page.
fn effective_size(ts: &TextState, ctm: &[f64; 6]) -> f64 {
    let c = ts.tm[2] * ctm[0] + ts.tm[3] * ctm[2];
    let d = ts.tm[2] * ctm[1] + ts.tm[3] * ctm[3];
    (ts.font_size * c.hypot(d)).abs()
}

fn fmt_matrix(m: &[f64; 6]) -> String {
    let items: Vec<String> = m.iter().map(|v| num(*v)).collect();
    format!("[{}]", items.join(" "))
}

/// A row for an enclosing sequence, linked when it has a node.
fn marked_row(group: PropGroup, page: PageIx, data: &PageData, seq: &MarkedSeq) -> PropGroup {
    let key = if seq.artifact.is_some() {
        "Artifact"
    } else {
        "Marked content"
    };
    let value = seq_summary(seq);
    match data.seq_node(page, seq) {
        Some(node) => group.link(key, value, node),
        None => group.row(key, value),
    }
}

/// Group of the sequences around an operator (innermost first), with the one it opens or closes.
fn marked_group(page: PageIx, data: &PageData, op: &OpRecord) -> Option<PropGroup> {
    let mut group = PropGroup::new("Marked content");
    let own = |seq_ix: &u32| data.seq(*seq_ix);
    if let Some(seq) = data.begin_of.get(&op.ix).and_then(own) {
        group = marked_row_as(group, "Begins", page, data, seq);
    }
    if let Some(seq) = data.end_of.get(&op.ix).and_then(own) {
        group = marked_row_as(group, "Ends", page, data, seq);
    }
    let enclosing: Vec<u32> = data
        .enclosing(op)
        .into_iter()
        .filter(|s| data.end_of.get(&op.ix) != Some(s))
        .collect();
    for seq in enclosing.iter().filter_map(own) {
        group = marked_row(group, page, data, seq);
    }
    (!group.rows.is_empty()).then_some(group)
}

fn marked_row_as(group: PropGroup, key: &str, page: PageIx, data: &PageData, seq: &MarkedSeq) -> PropGroup {
    let value = seq_summary(seq);
    match data.seq_node(page, seq) {
        Some(node) => group.link(key, value, node),
        None => group.row(key, value),
    }
}

/// Links to the first glyphs a text operator painted.
fn glyphs_group(page: PageIx, data: &PageData, op: &OpRecord) -> Option<PropGroup> {
    let glyphs: Vec<&PaintItem> = data.op_glyphs(op).collect();
    if glyphs.is_empty() {
        return None;
    }
    let mut group = PropGroup::new("Painted glyphs");
    for item in glyphs.iter().take(MAX_GLYPH_LINKS) {
        let PaintKind::Glyph {
            glyph_index, unicode, ..
        } = &item.kind
        else {
            continue;
        };
        let shown = unicode
            .as_deref()
            .filter(|u| !u.is_empty())
            .unwrap_or("(no unicode)");
        group = group.link(
            format!("#{glyph_index}"),
            shown,
            NodeId::Glyph {
                page,
                op: op.ix,
                glyph: *glyph_index,
            },
        );
    }
    if glyphs.len() > MAX_GLYPH_LINKS {
        group = group.row("More", format!("{} more glyphs", glyphs.len() - MAX_GLYPH_LINKS));
    }
    Some(group)
}

/// Begin / end operator rows of a sequence.
fn sequence_rows(group: PropGroup, page: PageIx, seq: &MarkedSeq) -> PropGroup {
    let group = group.link(
        "Begin operator",
        format!("{} (op {})", begin_label(seq), seq.begin_op),
        NodeId::Op {
            page,
            ix: seq.begin_op,
        },
    );
    match seq.end_op {
        Some(end) => group.link(
            "End operator",
            format!("EMC (op {end})"),
            NodeId::Op { page, ix: end },
        ),
        None => group.row("End operator", "unclosed"),
    }
}

fn begin_label(seq: &MarkedSeq) -> &'static str {
    if seq.properties.is_empty() && seq.property_resource.is_none() {
        "BMC"
    } else {
        "BDC"
    }
}

/// The property list of a sequence, key by key.
fn properties_group(seq: &MarkedSeq) -> PropGroup {
    let mut group = PropGroup::new("Properties");
    for (key, value) in &seq.properties {
        group = group.row(format!("/{key}"), value.clone());
    }
    group
}

/// The sequences enclosing `seq`, outermost last.
fn parents_group(page: PageIx, data: &PageData, seq: &MarkedSeq) -> Option<PropGroup> {
    let mut group = PropGroup::new("Enclosed by");
    for parent in data.seq_chain(seq.parent).iter().filter_map(|&s| data.seq(s)) {
        group = marked_row(group, page, data, parent);
    }
    (!group.rows.is_empty()).then_some(group)
}
