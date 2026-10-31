//! The operator walker: goes over a page's operators in order, keeps the capture's state up to
//! date through its collaborators, and turns the engine's per-operator events into
//! [`PaintItem`]s.
//!
//! The walker is the coordinator. State lives in small collaborators, each with a narrow API:
//!
//! * [`Engine`]: the engine edge (runs `hayro-interpret`, collects its warnings);
//! * [`GStateStack`]: the graphics state, its `q`/`Q` saves and the current font;
//! * [`PathBuilder`]: the current path and a pending `W` clip;
//! * [`MarkedContent`]: marked-content sequences and the open stack;
//! * [`XObjectScopes`]: drawn Form `XObject` invocations and the recursion guard;
//! * [`FontCache`]: one font reading per indirect font dictionary.
//!
//! Geometry inside the walker is core [`Matrix`] / [`Rect`] / [`Point`]. `kurbo` appears only
//! where the engine hands geometry over (glyph transforms in [`super::text`], the
//! [`Engine`] context) and inside [`PathBuilder`], which needs exact curve bounds. The public
//! data model keeps its six-number arrays; the walker converts at that boundary.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;

use hayro_syntax::Pdf;
use hayro_syntax::object::{Array, Dict, Name, Object, Stream};
use hayro_syntax::page::{Page, Resources};
use pdfxray_raw::ObjRef;

use super::color::{color_space_info, device_operator};
use super::degrade::DegradeReason;
use super::engine::Engine;
use super::font::FontInfo;
use super::font_cache::FontCache;
use super::geom::{intersect, transform_bbox, union};
use super::gstate::GStateStack;
use super::marked::{MarkedContent, Opening};
use super::object::{name_token, object_syntax};
use super::ops::{ParsedOps, RawOp, instrument, is_painting, name_at, parse_ops};
use super::path::PathBuilder;
use super::recorder::{Event, Group, Recorder, RunGlyph};
use super::scopes::{ScopeStart, XObjectScopes};
use super::text::{ShownText, check_first_origin, glyph_geometry, inflate_stroke, translation};
use super::types::{
    Alignment, CaptureNote, CapturedPage, ColorInfo, GState, OpRecord, PaintItem, PaintKind, StreamRef,
};
use crate::numeric::u8_from_f64_trunc;
use crate::{Matrix, Rect};

/// Deepest Form `XObject` nesting followed; the engine itself stops at 50.
const MAX_FORM_DEPTH: usize = 48;

/// Where the operators being walked come from.
struct StreamInput<'d> {
    data: &'d [u8],
    /// Offsets in `data` where each page `/Contents` stream starts (page content only).
    seg_starts: &'d [usize],
    /// The stream identity for Form `XObjects`; `None` for page content.
    form: Option<StreamRef>,
}

impl StreamInput<'_> {
    /// Stream identity and the stream-local offset for the byte at `pos`.
    fn locate(&self, pos: usize) -> (StreamRef, usize) {
        if let Some(form) = &self.form {
            return (form.clone(), pos);
        }
        let (idx, offset) = self.segment(pos);
        let index = u32::try_from(idx).unwrap_or(u32::MAX);
        (StreamRef::PageContent { index }, offset)
    }

    /// The stream-local offset of the byte at `pos`.
    fn offset(&self, pos: usize) -> usize {
        if self.form.is_some() {
            return pos;
        }
        self.segment(pos).1
    }

    /// The `/Contents` stream holding the byte at `pos`, and the offset of `pos` in it.
    fn segment(&self, pos: usize) -> (usize, usize) {
        let idx = self.seg_starts.partition_point(|s| *s <= pos).saturating_sub(1);
        let base = self.seg_starts.get(idx).copied().unwrap_or(0);
        (idx, pos.saturating_sub(base))
    }
}

/// A Form `XObject` about to be entered.
struct FormEntry<'a> {
    name: String,
    obj: Option<ObjRef>,
    decoded: Cow<'a, [u8]>,
    matrix: Matrix,
    /// `/BBox` as written: it is reported unnormalised, so it stays the PDF array.
    bbox: Option<[f64; 4]>,
    resources: Resources<'a>,
    /// Set when the form shows text in an inherited font it cannot select.
    font_gap: Option<DegradeReason>,
}

pub(super) struct Walker<'a> {
    engine: Engine<'a>,
    page_ix: u32,

    ops: Vec<OpRecord>,
    items: Vec<PaintItem>,
    notes: Vec<CaptureNote>,
    /// The first problem found; later ones are not recorded.
    degraded: Option<DegradeReason>,

    state: GStateStack,
    path: PathBuilder,
    marked: MarkedContent,
    scopes: XObjectScopes,
    fonts: FontCache,
    /// Union of the glyph boxes of clipping text in the current text object, applied at `ET`.
    text_clip: Option<Rect>,
    /// Set while a form shows text in a font it inherited but cannot select.
    font_gap: Option<DegradeReason>,
}

/// Captures one page. Never panics: a panic inside the engine yields a degraded page holding
/// whatever was captured before it.
pub(super) fn capture(pdf: &Pdf, page_ix: u32, page: &Page<'_>) -> CapturedPage {
    let media = page.media_box();
    let mut walker = Walker {
        engine: Engine::new(pdf, kurbo::Rect::new(media.x0, media.y0, media.x1, media.y1)),
        page_ix,
        ops: Vec::new(),
        items: Vec::new(),
        notes: Vec::new(),
        degraded: None,
        state: GStateStack::default(),
        path: PathBuilder::default(),
        marked: MarkedContent::default(),
        scopes: XObjectScopes::default(),
        fonts: FontCache::default(),
        text_clip: None,
        font_gap: None,
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| walker.run(page)));
    if outcome.is_err() {
        walker.degrade(DegradeReason::PagePanicked);
    }
    walker.finish()
}

/// Concatenates the page's content streams exactly as the engine does, remembering where each
/// stream starts.
fn page_content(page: &Page<'_>) -> (Vec<u8>, Vec<usize>) {
    let decode = |s: Stream<'_>| s.decoded().ok().map(|d| d.to_vec());
    if let Some(stream) = page.raw().get::<Stream<'_>>(b"Contents") {
        return (decode(stream).unwrap_or_default(), vec![0]);
    }
    let mut data = Vec::new();
    let mut starts = Vec::new();
    if let Some(array) = page.raw().get::<Array<'_>>(b"Contents") {
        for stream in array.iter::<Stream<'_>>() {
            if let Some(bytes) = decode(stream) {
                starts.push(data.len());
                data.extend(bytes);
                data.push(b' ');
            }
        }
    }
    if starts.is_empty() {
        starts.push(0);
    }
    (data, starts)
}

fn raw_ref(r: hayro_syntax::object::ObjRef) -> ObjRef {
    ObjRef::new(
        u32::try_from(r.obj_number).unwrap_or(0),
        u16::try_from(r.gen_number).unwrap_or(0),
    )
}

/// Number operand `i` of `v`, or 0.
fn at(v: &[f64], i: usize) -> f64 {
    v.get(i).copied().unwrap_or(0.0)
}

/// `v` truncated into `0..=255`; NaN and the infinities are 0.
fn clamp_u8(v: f64) -> u8 {
    if v.is_finite() { u8_from_f64_trunc(v) } else { 0 }
}

/// True when `op` is a `Do` of a Form `XObject` in `resources`.
fn is_form(resources: &Resources<'_>, op: &RawOp<'_>) -> bool {
    let Some(name) = name_at(op, 0) else {
        return false;
    };
    resources
        .get_x_object(name)
        .and_then(|s| s.dict().get::<Name<'_>>(b"Subtype").map(|n| n.as_str() == "Form"))
        .unwrap_or(false)
}

impl<'a> Walker<'a> {
    fn run(&mut self, page: &Page<'a>) {
        let (data, seg_starts) = page_content(page);
        let input = StreamInput {
            data: &data,
            seg_starts: &seg_starts,
            form: None,
        };
        self.walk_stream(&input, page.resources(), Matrix::IDENTITY, 0);
    }

    fn finish(self) -> CapturedPage {
        let mut notes = self.notes;
        notes.extend(self.engine.take_warnings());
        // The vectors grew by doubling and are kept for the life of the document: give back the
        // unused tail, which is a fifth of the memory of a text-heavy page.
        let (mut ops, mut items, mut marked) = (self.ops, self.items, self.marked.into_sequences());
        ops.shrink_to_fit();
        items.shrink_to_fit();
        marked.shrink_to_fit();
        CapturedPage {
            page: self.page_ix,
            ops,
            items,
            marked,
            xobjects: self.scopes.into_scopes(),
            alignment: match self.degraded {
                None => Alignment::Exact,
                Some(reason) => Alignment::Degraded {
                    reason: reason.to_string(),
                },
            },
            notes,
        }
    }

    fn degrade(&mut self, reason: DegradeReason) {
        if self.degraded.is_none() {
            self.degraded = Some(reason);
        }
    }

    fn note(&mut self, op: Option<u32>, message: impl Into<String>) {
        self.notes.push(CaptureNote {
            op,
            message: message.into(),
        });
    }

    // ---------------------------------------------------------------------------------------
    // Engine run
    // ---------------------------------------------------------------------------------------

    /// Runs the engine over the instrumented stream and returns one event group per operator.
    fn run_engine(
        &mut self,
        input: &StreamInput<'_>,
        parsed: &ParsedOps<'_>,
        resources: &Resources<'a>,
        base_ctm: Matrix,
    ) -> Vec<Group> {
        let count = parsed.ops.len();
        if !parsed.spans_ok {
            self.degrade(DegradeReason::BoundariesLost);
            return vec![Group::default(); count];
        }
        if let Some(from) = parsed.unparsed_from {
            self.degrade(DegradeReason::Untokenizable { from });
        }
        let discard: Vec<bool> = parsed
            .ops
            .iter()
            .map(|op| op.operator == "Do" && is_form(resources, op))
            .collect();
        let prologue = if input.form.is_some() {
            self.text_prologue(resources)
        } else {
            String::new()
        };
        let instrumented = instrument(input.data, parsed, &prologue);
        let mut recorder = Recorder::new(count, discard);
        if !self
            .engine
            .interpret(&instrumented, resources, base_ctm, &mut recorder)
        {
            self.degrade(DegradeReason::StreamPanicked {
                after: recorder.reached,
            });
        }
        if recorder.stray > 0 {
            self.degrade(DegradeReason::StrayContent);
        }
        if recorder.reached < count {
            let skipped = &parsed.ops[recorder.reached..];
            if let Some(op) = skipped.iter().find(|op| is_painting(&op.operator)) {
                self.degrade(DegradeReason::EngineStopped {
                    at: recorder.reached,
                    count,
                    operator: op.operator.clone(),
                });
            }
        }
        recorder.groups
    }

    /// Operators restoring text state the engine would otherwise not inherit into a separate
    /// run of a Form `XObject`.
    fn text_prologue(&self, resources: &Resources<'a>) -> String {
        let t = &self.state.current().text;
        let mode = if t.render_mode == 7 { 3 } else { t.render_mode };
        let mut out = format!(
            "{} Tc {} Tw {} Tz {} TL {} Ts {} Tr\n",
            t.char_spacing, t.word_spacing, t.horizontal_scaling, t.leading, t.rise, mode
        );
        if let Some(font) = &t.font {
            let known = Name::new(font.as_bytes()).is_some_and(|n| resources.get_font(&n).is_some());
            if known {
                let _ = writeln!(out, "{} {} Tf", name_token(font), t.font_size);
            }
        }
        out
    }

    // ---------------------------------------------------------------------------------------
    // Stream walk
    // ---------------------------------------------------------------------------------------

    fn walk_stream(
        &mut self,
        input: &StreamInput<'_>,
        resources: &Resources<'a>,
        base_ctm: Matrix,
        depth: usize,
    ) {
        if depth > MAX_FORM_DEPTH {
            self.note(None, "Form XObject nesting too deep; inner content skipped");
            return;
        }
        let parsed = parse_ops(input.data);
        self.ops.reserve(parsed.ops.len());
        let mut groups = self.run_engine(input, &parsed, resources, base_ctm).into_iter();

        for (local_ix, op) in parsed.ops.into_iter().enumerate() {
            let ix = u32::try_from(self.ops.len()).unwrap_or(u32::MAX);
            let (stream, local_start) = input.locate(op.start);
            let local_end = input.offset(op.end.max(op.start));
            let group = groups.next().unwrap_or_default();

            let pre = self.state.snapshot();
            let items_before = self.items.len();
            let form = self.exec(ix, &op, resources, group, &pre, &stream);
            let item_count = self.items.len() - items_before;
            // Only a drawn form needs the invoking stream once the record owns it.
            let parent = form.is_some().then(|| stream.clone());

            let record = OpRecord {
                ix,
                operands: op.operands.iter().map(object_syntax).collect(),
                operator: op.operator,
                stream,
                local_ix: u32::try_from(local_ix).unwrap_or(u32::MAX),
                span: Some((local_start, local_end.max(local_start))),
                mc_depth: u32::try_from(self.marked.open().len()).unwrap_or(u32::MAX),
                xobject: self.scopes.innermost(),
                first_item: (item_count > 0).then(|| u32::try_from(items_before).unwrap_or(u32::MAX)),
                item_count: u32::try_from(item_count).unwrap_or(u32::MAX),
                gstate: self.state.snapshot(),
            };
            self.ops.push(record);

            if let Some((entry, parent)) = form.zip(parent) {
                self.enter_form(ix, entry, parent, depth);
            }
        }
    }

    fn enter_form(&mut self, op_ix: u32, entry: FormEntry<'a>, parent: StreamRef, depth: usize) {
        if let Some(obj) = entry.obj
            && self.scopes.is_active(obj)
        {
            self.note(
                Some(op_ix),
                format!("Form XObject {} invokes itself; recursion skipped", entry.name),
            );
            return;
        }
        let outer_gap = std::mem::replace(&mut self.font_gap, entry.font_gap);
        let inner_ctm = entry.matrix.concat(&self.state.ctm());
        let page_bbox = entry.bbox.and_then(|b| transform_bbox(inner_ctm, b));
        let stream = StreamRef::FormXObject {
            obj: entry.obj,
            name: entry.name.clone(),
            parent: Box::new(parent),
        };
        let scope_ix = self.scopes.enter(ScopeStart {
            name: entry.name,
            obj: entry.obj,
            op: op_ix,
            matrix: entry.matrix,
            bbox: entry.bbox,
            ctm: inner_ctm,
            page_bbox,
            stream: stream.clone(),
        });

        let outer_state = self.state.enter_form();
        let outer_marks = self.marked.enter_form();
        self.state.edit().ctm = inner_ctm.to_array();
        if let Some(bbox) = page_bbox {
            self.clip_to(bbox);
        }
        self.path.reset();
        let first = self.ops.len();

        let input = StreamInput {
            data: &entry.decoded,
            seg_starts: &[0],
            form: Some(stream),
        };
        self.walk_stream(&input, &entry.resources, inner_ctm, depth + 1);

        self.scopes.leave(scope_ix, first..self.ops.len());
        self.marked.leave_form(outer_marks);
        self.state.leave_form(outer_state);
        self.font_gap = outer_gap;
    }

    /// A form may use the font selected before its `Do` without listing it in its own resources.
    /// The engine run over the form needs a resource entry to select that font again, so the
    /// invoker's font dictionary stands in when the form has none of its own. Returns why the
    /// form's glyphs are unreliable when it cannot.
    fn inherit_font(
        &self,
        op_ix: u32,
        form_name: &str,
        form: &mut Resources<'a>,
        invoker: &Resources<'a>,
    ) -> Option<DegradeReason> {
        let font = self.state.current().text.font.as_deref()?;
        let name = Name::new(font.as_bytes())?;
        if form.get_font(&name).is_some() {
            return None;
        }
        if form.fonts.is_empty() && invoker.fonts.contains_key(font.as_bytes()) {
            form.fonts = invoker.fonts.clone();
            None
        } else {
            Some(DegradeReason::InheritedFontUndefined {
                op: op_ix,
                form: form_name.to_owned(),
                font: font.to_owned(),
            })
        }
    }

    // ---------------------------------------------------------------------------------------
    // Items
    // ---------------------------------------------------------------------------------------

    fn push_item(&mut self, op: u32, kind: PaintKind, bbox: Rect, gstate: &Arc<GState>) {
        let ix = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        self.marked.painted(bbox);
        self.items.push(PaintItem {
            ix,
            op,
            kind,
            bbox,
            clip: gstate.clip,
            gstate: Arc::clone(gstate),
            mc_stack: self.marked.open().to_vec(),
            xobject_stack: self.scopes.open().to_vec(),
        });
    }

    /// Narrows the clip in effect by `bbox`.
    fn clip_to(&mut self, bbox: Rect) {
        let clipped = self.state.current().clip.map_or(bbox, |c| intersect(c, bbox));
        self.state.edit().clip = Some(clipped);
    }

    /// Ends the current path; a pending `W` turns it into a clip item.
    fn end_path(&mut self, ix: u32, pre: &Arc<GState>) {
        if let Some(bbox) = self.path.end(self.state.ctm()) {
            self.push_item(ix, PaintKind::Clip, bbox, pre);
            self.clip_to(bbox);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Operators
    // ---------------------------------------------------------------------------------------

    /// Applies one operator to the state and creates the items it painted from `group`, the
    /// engine's events for it. Returns the Form `XObject` to descend into, if the operator was
    /// a `Do` of a drawn form.
    fn exec(
        &mut self,
        ix: u32,
        op: &RawOp<'_>,
        resources: &Resources<'a>,
        group: Group,
        pre: &Arc<GState>,
        stream: &StreamRef,
    ) -> Option<FormEntry<'a>> {
        let v = op.numbers();
        match op.operator.as_str() {
            "q" => self.state.save(),
            "Q" => self.restore_state(ix),
            "cm" if v.len() >= 6 => {
                let m = Matrix::from([v[0], v[1], v[2], v[3], v[4], v[5]]);
                let ctm = m.concat(&self.state.ctm());
                self.state.edit().ctm = ctm.to_array();
            }
            "w" => self.state.edit().line_width = at(&v, 0),
            "J" => self.state.edit().line_cap = clamp_u8(at(&v, 0)),
            "j" => self.state.edit().line_join = clamp_u8(at(&v, 0)),
            "M" => self.state.edit().miter_limit = at(&v, 0),
            "i" => self.state.edit().flatness = at(&v, 0),
            "ri" => self.state.edit().rendering_intent = name_at(op, 0).map(|n| n.as_str().to_owned()),
            "d" => {
                if let Some(Object::Array(a)) = op.operands.first() {
                    let dash: Vec<f64> = a.iter::<f64>().collect();
                    let phase = at(&v, 0);
                    let gs = self.state.edit();
                    gs.dash_array = dash;
                    gs.dash_phase = phase;
                }
            }
            "gs" => self.set_ext_gstate(op, resources),
            "m" | "l" | "c" | "v" | "y" | "h" | "re" => self.path.construct(op.operator.as_str(), &v),
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                if matches!(op.operator.as_str(), "s" | "b" | "b*") {
                    self.path.close_subpath();
                }
                let kind = match op.operator.as_str() {
                    "S" | "s" => PaintKind::Stroke,
                    "f" | "F" | "f*" => PaintKind::Fill,
                    _ => PaintKind::FillStroke,
                };
                if let Some(bbox) = group.path_box() {
                    self.push_item(ix, kind, bbox, pre);
                }
                self.end_path(ix, pre);
            }
            "n" => self.end_path(ix, pre),
            "W" | "W*" => self.path.clip(),
            "BT" => {
                let text = &mut self.state.edit().text;
                text.tm = Matrix::IDENTITY.to_array();
                text.tlm = Matrix::IDENTITY.to_array();
                self.text_clip = None;
            }
            "ET" => {
                if let Some(bbox) = self.text_clip.take() {
                    self.push_item(ix, PaintKind::Clip, bbox, pre);
                    self.clip_to(bbox);
                }
            }
            "Tc" => self.state.edit().text.char_spacing = at(&v, 0),
            "Tw" => self.state.edit().text.word_spacing = at(&v, 0),
            "Tz" => self.state.edit().text.horizontal_scaling = at(&v, 0),
            "TL" => self.state.edit().text.leading = at(&v, 0),
            "Ts" => self.state.edit().text.rise = at(&v, 0),
            "Tr" => self.state.edit().text.render_mode = clamp_u8(at(&v, 0)).min(7),
            "Tf" => self.set_font(op, resources),
            "Td" => self.move_text(at(&v, 0), at(&v, 1)),
            "TD" => {
                self.state.edit().text.leading = -at(&v, 1);
                self.move_text(at(&v, 0), at(&v, 1));
            }
            "T*" => self.next_line(),
            "Tm" if v.len() >= 6 => {
                let m = [v[0], v[1], v[2], v[3], v[4], v[5]];
                let text = &mut self.state.edit().text;
                text.tm = m;
                text.tlm = m;
            }
            "Tj" | "TJ" => self.show_text(ix, op, group, pre),
            "'" => {
                self.next_line();
                let pre = self.state.snapshot();
                self.show_text(ix, op, group, &pre);
            }
            "\"" => {
                let text = &mut self.state.edit().text;
                text.word_spacing = at(&v, 0);
                text.char_spacing = at(&v, 1);
                self.next_line();
                let pre = self.state.snapshot();
                self.show_text(ix, op, group, &pre);
            }
            "g" | "G" | "rg" | "RG" | "k" | "K" => {
                let (space, stroke) = device_operator(op.operator.as_str());
                self.set_color(stroke, ColorInfo::device(space, &v));
            }
            "cs" | "CS" => {
                if let Some(name) = name_at(op, 0) {
                    let info = color_space_info(resources, name);
                    self.set_color(op.operator == "CS", info);
                }
            }
            "sc" | "scn" | "SC" | "SCN" => {
                let stroke = op.operator.starts_with('S');
                let pattern = op.operands.last().and_then(|o| match o {
                    Object::Name(n) => Some(n.as_str().to_owned()),
                    _ => None,
                });
                let gs = self.state.edit();
                let target = if stroke { &mut gs.stroke } else { &mut gs.fill };
                target.components = v;
                target.pattern = pattern;
            }
            "sh" => {
                if let Some(bbox) = group.path_box() {
                    let limit = pre
                        .clip
                        .unwrap_or_else(|| self.engine.media_box().unwrap_or(bbox));
                    self.push_item(ix, PaintKind::Shading, intersect(bbox, limit), pre);
                }
            }
            "Do" => return self.do_xobject(ix, op, resources, &group, pre),
            "BI" => self.inline_image(ix, &group, pre),
            "BMC" | "BDC" => self
                .marked
                .begin(Opening::read(op, resources), ix, stream.clone()),
            "EMC" => self.end_marked(ix),
            _ => {}
        }
        None
    }

    fn restore_state(&mut self, ix: u32) {
        if !self.state.restore() {
            self.note(Some(ix), "`Q` without a matching `q` in this stream ignored");
        }
    }

    fn end_marked(&mut self, ix: u32) {
        if !self.marked.end(ix) {
            self.note(
                Some(ix),
                "`EMC` without a matching `BMC`/`BDC` in this stream ignored",
            );
        }
    }

    fn set_ext_gstate(&mut self, op: &RawOp<'_>, resources: &Resources<'a>) {
        let Some(name) = name_at(op, 0) else {
            return;
        };
        self.state.edit().ext_gstate = Some(name.as_str().to_owned());
        let Some(dict) = resources.get_ext_g_state(name) else {
            return;
        };
        for key in dict.keys() {
            match key.as_str() {
                "LW" => self.state.edit().line_width = dict.get::<f64>(&key).unwrap_or(1.0),
                "LC" => self.state.edit().line_cap = clamp_u8(dict.get::<f64>(&key).unwrap_or(0.0)),
                "LJ" => self.state.edit().line_join = clamp_u8(dict.get::<f64>(&key).unwrap_or(0.0)),
                "ML" => self.state.edit().miter_limit = dict.get::<f64>(&key).unwrap_or(10.0),
                "FL" => self.state.edit().flatness = dict.get::<f64>(&key).unwrap_or(0.0),
                "RI" => {
                    self.state.edit().rendering_intent =
                        dict.get::<Name<'_>>(&key).map(|n| n.as_str().to_owned());
                }
                "CA" => self.state.edit().alpha_stroke = dict.get::<f64>(&key).unwrap_or(1.0),
                "ca" => self.state.edit().alpha_fill = dict.get::<f64>(&key).unwrap_or(1.0),
                "BM" => {
                    let mode = dict
                        .get::<Name<'_>>(&key)
                        .map(|n| n.as_str().to_owned())
                        .or_else(|| {
                            dict.get::<Array<'_>>(&key)
                                .and_then(|a| a.iter::<Name<'_>>().next())
                                .map(|n| n.as_str().to_owned())
                        })
                        .unwrap_or_else(|| "Normal".to_owned());
                    self.state.edit().blend_mode = mode;
                }
                "SMask" => {
                    let active = dict.get::<Name<'_>>(&key).is_none() && dict.get::<Dict<'_>>(&key).is_some();
                    self.state.edit().soft_mask = active;
                }
                "D" => {
                    if let Some(arr) = dict.get::<Array<'_>>(&key) {
                        let mut it = arr.iter::<Object<'_>>();
                        if let (Some(Object::Array(d)), Some(Object::Number(p))) = (it.next(), it.next()) {
                            let dash: Vec<f64> = d.iter::<f64>().collect();
                            let gs = self.state.edit();
                            gs.dash_array = dash;
                            gs.dash_phase = p.as_f64();
                        }
                    }
                }
                "Font" => {
                    if let Some(arr) = dict.get::<Array<'_>>(&key) {
                        let font_ref = arr.raw_iter().next().and_then(|m| m.as_obj_ref()).map(raw_ref);
                        let size = match arr.iter::<Object<'_>>().nth(1) {
                            Some(Object::Number(n)) => Some(n.as_f64()),
                            _ => None,
                        };
                        if let Some(font_dict) = arr.iter::<Dict<'_>>().next() {
                            let font = FontInfo::from_dict(&font_dict, self.engine.settings());
                            self.state.set_font(Rc::new(font));
                        }
                        let text = &mut self.state.edit().text;
                        text.font = None;
                        text.font_ref = font_ref;
                        if let Some(size) = size {
                            text.font_size = size;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn set_color(&mut self, stroke: bool, info: ColorInfo) {
        let gs = self.state.edit();
        if stroke {
            gs.stroke = info;
        } else {
            gs.fill = info;
        }
    }

    // ---------------------------------------------------------------------------------------
    // Text
    // ---------------------------------------------------------------------------------------

    fn set_font(&mut self, op: &RawOp<'_>, resources: &Resources<'a>) {
        let Some(name) = name_at(op, 0) else {
            return;
        };
        let size = op
            .operands
            .iter()
            .rev()
            .find_map(|o| match o {
                Object::Number(n) => Some(n.as_f64()),
                _ => None,
            })
            .unwrap_or(0.0);
        let font_ref = resources.fonts.get_ref(name.as_ref()).map(raw_ref);
        let font = self
            .fonts
            .resolve(font_ref, resources.get_font(name), self.engine.settings());
        self.state.set_font(font);
        self.font_gap = None;
        let text = &mut self.state.edit().text;
        text.font = Some(name.as_str().to_owned());
        text.font_ref = font_ref;
        text.font_size = size;
    }

    fn move_text(&mut self, tx: f64, ty: f64) {
        let text = &mut self.state.edit().text;
        let line = translation(tx, ty).concat(&Matrix::from(text.tlm)).to_array();
        text.tlm = line;
        text.tm = line;
    }

    /// `T*`, and the line move of `'` and `"`.
    fn next_line(&mut self) {
        let leading = self.state.current().text.leading;
        self.move_text(0.0, -leading);
    }

    fn show_text(&mut self, ix: u32, op: &RawOp<'_>, group: Group, pre: &Arc<GState>) {
        // A shared handle: the font stays readable while items are pushed.
        let font = self
            .state
            .font()
            .map_or_else(|| Rc::new(FontInfo::fallback()), Rc::clone);
        // The gap stays set for the rest of the form; only the first degradation is kept.
        if self.degraded.is_none()
            && let Some(gap) = &self.font_gap
        {
            self.degraded = Some(gap.clone());
        }
        let shown = ShownText::read(op, &font);
        let mut run: Option<Vec<RunGlyph>> = group.events.into_iter().find_map(|e| match e {
            Event::Glyphs { glyphs } => Some(glyphs),
            _ => None,
        });
        let t = &pre.text;

        if let Some(glyphs) = &mut run {
            let paired = glyphs.len() == shown.codes.len();
            if !paired {
                self.degrade(DegradeReason::GlyphCountMismatch {
                    op: ix,
                    glyphs: glyphs.len(),
                    codes: shown.codes.len(),
                });
            }
            if let Some(reason) = glyphs
                .first()
                .and_then(|first| check_first_origin(ix, pre, shown.lead, first))
            {
                self.degrade(reason);
            }
            let stroked = matches!(t.render_mode, 1 | 2 | 5 | 6);
            let clips = t.render_mode >= 4;
            for (k, glyph) in glyphs.iter_mut().enumerate() {
                let code = if paired { shown.codes[k].0 } else { 0 };
                let width = glyph
                    .advance
                    .or_else(|| font.type3_widths.as_ref().and_then(|w| w.width(code)))
                    .unwrap_or(0.0);
                let (bbox, origin, advance) = glyph_geometry(&font, glyph, width, font.type3_box(code));
                let bbox = if stroked { inflate_stroke(bbox, pre) } else { bbox };
                if clips {
                    self.text_clip = Some(self.text_clip.map_or(bbox, |c| union(c, bbox)));
                }
                let kind = PaintKind::Glyph {
                    glyph_index: u32::try_from(k).unwrap_or(u32::MAX),
                    char_code: code,
                    // The run is the operator's own; its text moves into the item.
                    unicode: glyph.unicode.take(),
                    origin,
                    advance,
                };
                self.push_item(ix, kind, bbox, pre);
            }
        } else {
            self.note(
                Some(ix),
                "text operator produced no glyphs (empty string, hidden by optional content, or unusable font)",
            );
        }

        let dx = shown.advance(t, run.as_deref(), &font);
        let text = &mut self.state.edit().text;
        text.tm = translation(dx, 0.0).concat(&Matrix::from(text.tm)).to_array();
    }

    // ---------------------------------------------------------------------------------------
    // XObjects and images
    // ---------------------------------------------------------------------------------------

    fn do_xobject(
        &mut self,
        ix: u32,
        op: &RawOp<'_>,
        resources: &Resources<'a>,
        group: &Group,
        pre: &Arc<GState>,
    ) -> Option<FormEntry<'a>> {
        let name = name_at(op, 0)?;
        let Some(stream) = resources.get_x_object(name) else {
            self.note(
                Some(ix),
                format!(
                    "XObject {} could not be resolved (missing from the resources, or a reference cycle)",
                    name.as_str()
                ),
            );
            return None;
        };
        let subtype = stream.dict().get::<Name<'_>>(b"Subtype");
        match subtype.as_ref().map(Name::as_str) {
            Some("Image") => {
                if let Some((bbox, width, height)) = group.image() {
                    let kind = PaintKind::Image {
                        width,
                        height,
                        inline: false,
                    };
                    self.push_item(ix, kind, bbox, pre);
                } else {
                    self.note(
                        Some(ix),
                        format!(
                            "image {} was not drawn (hidden by optional content or undecodable)",
                            name.as_str()
                        ),
                    );
                }
                None
            }
            Some("Form") if group.touched => {
                let dict = stream.dict();
                let decoded = stream.decoded().ok()?;
                let mut form_resources = dict
                    .get::<Dict<'_>>(b"Resources")
                    .map_or_else(|| resources.clone(), Resources::new);
                let form_name = name.as_str().to_owned();
                let font_gap = self.inherit_font(ix, &form_name, &mut form_resources, resources);
                Some(FormEntry {
                    name: form_name,
                    obj: resources.x_objects.get_ref(name.as_ref()).map(raw_ref),
                    decoded,
                    matrix: dict
                        .get::<[f64; 6]>(b"Matrix")
                        .map_or(Matrix::IDENTITY, Matrix::from),
                    bbox: dict.get::<[f64; 4]>(b"BBox"),
                    resources: form_resources,
                    font_gap,
                })
            }
            Some("Form") => {
                self.note(Some(ix), format!("form {} was not drawn", name.as_str()));
                None
            }
            _ => None,
        }
    }

    fn inline_image(&mut self, ix: u32, group: &Group, pre: &Arc<GState>) {
        if let Some((bbox, width, height)) = group.image() {
            let kind = PaintKind::Image {
                width,
                height,
                inline: true,
            };
            self.push_item(ix, kind, bbox, pre);
        } else {
            self.note(
                Some(ix),
                "inline image was not drawn (hidden by optional content or undecodable)",
            );
        }
    }
}
