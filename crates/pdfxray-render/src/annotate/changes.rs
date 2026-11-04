//! A diff as [`Scene`]s: **the status takes the colour, the role moves to the chip text**, and a
//! [`Legend`] says what each colour means. Role colours would collide with status colours on the
//! same box, so a diff never uses them.
//!
//! Side by side, each side outlines its own elements. Overlaid (or as a mask) the two sides share
//! one picture, so an element is outlined once: where B has it, except a removed element (only A
//! has it) and the place a moved element left, which A outlines faintly.

use pdfxray_core::diff::{Pair, Pairing, Side, Status};
use pdfxray_core::{NodeId, PageIx, Rect, RoleClass, TagBox};

use super::{Chip, Corner, Legend, Look, Mark, Scene, chip_text};
use crate::composite::{Mode, TINT_A, TINT_B};
use crate::score::{INK_ADDED, INK_REMOVED, VisualScore};

/// The colour of `status`.
pub const fn status_rgba(status: Status) -> [u8; 4] {
    match status {
        Status::Same => [0x8A, 0x93, 0xA3, 255],
        Status::Modified => [0xF5, 0x9E, 0x0B, 255],
        Status::Moved => [0x8B, 0x5C, 0xF6, 255],
        Status::Added => [0x22, 0xC5, 0x5E, 255],
        Status::Removed => [0xEF, 0x44, 0x44, 255],
    }
}

/// `status` in words, as the legend and the JSON say it.
pub const fn status_name(status: Status) -> &'static str {
    match status {
        Status::Same => "same",
        Status::Modified => "modified",
        Status::Moved => "moved",
        Status::Added => "added",
        Status::Removed => "removed",
    }
}

/// Strength of the box a moved element left, overlaid.
const LEFT_BEHIND: f32 = 0.5;

/// What a diff's scenes show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangeShow {
    /// Unchanged elements too, in the `same` colour (*All tag boxes*).
    pub unchanged: bool,
    /// A chip naming each element's role (*Tag labels*).
    pub labels: bool,
    /// Containers too (*Nesting*).
    pub nesting: bool,
}

/// The scenes of page `page_a` of A and page `page_b` of B of `pairing`, each in its own side's
/// page space, shown as `mode` shows them. `tag_of` finds an element's box on its side's page:
/// it tells containers apart, and a chip names the type of its own side's element (a heading
/// retagged from `H2` to `H3` says `H2` on A and `H3` on B), else the pair's role.
pub fn change_scenes<'a>(
    pairing: &Pairing,
    (page_a, page_b): (Option<PageIx>, Option<PageIx>),
    mode: Mode,
    show: ChangeShow,
    tag_of: &dyn Fn(Side, &NodeId) -> Option<&'a TagBox>,
) -> [Scene; 2] {
    let shown = |pair: &Pair| pair.status != Status::Same || show.unchanged;
    let mut scenes = [Scene::default(), Scene::default()];
    for pair in pairing.pairs.iter().filter(|p| shown(p)) {
        let on_a = on(pair.a.as_ref(), pair.page_a, pair.bbox_a, page_a);
        let on_b = on(pair.b.as_ref(), pair.page_b, pair.bbox_b, page_b);
        let both = mode == Mode::SideBySide;
        if let Some((node, rect)) = on_a {
            let left_behind = !both && pair.status == Status::Moved;
            let tag = tag_of(Side::A, node);
            let container = is_container(tag);
            if (both || pair.status == Status::Removed || left_behind) && (!container || show.nesting) {
                let chip = (show.labels && !left_behind).then(|| label(pair, tag));
                let mut mark = mark(pair, node, rect, container, chip);
                if left_behind {
                    mark.look = Look::Container;
                    mark.strength = LEFT_BEHIND;
                }
                scenes[0].marks.push(mark);
            }
        }
        if let Some((node, rect)) = on_b
            && pair.status != Status::Removed
        {
            let tag = tag_of(Side::B, node);
            let container = is_container(tag);
            if !container || show.nesting {
                let chip = show.labels.then(|| label(pair, tag));
                scenes[1].marks.push(mark(pair, node, rect, container, chip));
            }
        }
    }
    scenes
}

fn is_container(tag: Option<&TagBox>) -> bool {
    tag.is_some_and(|t| t.class == RoleClass::Group)
}

/// What the chip of a side's element says: its own type, else the pair's role.
fn label(pair: &Pair, tag: Option<&TagBox>) -> String {
    tag.map_or_else(|| pair.role.clone(), |t| chip_text(t).to_owned())
}

/// The element and box of one side of a pair when they are on `page`.
fn on(
    node: Option<&NodeId>,
    at: Option<PageIx>,
    rect: Option<Rect>,
    page: Option<PageIx>,
) -> Option<(&NodeId, Rect)> {
    (at.is_some() && at == page).then_some(())?;
    Some((node?, rect?))
}

fn mark(pair: &Pair, node: &NodeId, rect: Rect, container: bool, chip: Option<String>) -> Mark {
    Mark {
        node: node.clone(),
        rect,
        color: status_rgba(pair.status),
        look: if container { Look::Container } else { Look::Solid },
        outlined: true,
        strength: 1.0,
        chip: chip.map(|text| Chip {
            text,
            corner: if container {
                Corner::TopRight
            } else {
                Corner::TopLeft
            },
            scope: None,
        }),
    }
}

/// The legend of a diff shown as `mode` shows it: a row per status drawn, the two ink colours
/// of an overlay, the two of a mask with its `visual` score, and one row per `warning` (a
/// degraded page, a registration) in `warn`.
pub fn change_legend(
    mode: Mode,
    show: ChangeShow,
    visual: Option<&VisualScore>,
    warnings: &[String],
    warn: [u8; 4],
) -> Legend {
    let mut rows: Vec<([u8; 4], String)> = Vec::new();
    if mode == Mode::Overlay {
        rows.push((TINT_A, "ink of A".to_owned()));
        rows.push((TINT_B, "ink of B".to_owned()));
    }
    if mode == Mode::Mask
        && let Some(visual) = visual
    {
        rows.push((INK_ADDED, "ink B adds".to_owned()));
        rows.push((INK_REMOVED, "ink B removes".to_owned()));
        rows.push((status_rgba(Status::Same), format!("visual: {}", visual.summary())));
        if let Some(aa) = visual.anti_aliasing() {
            rows.push((status_rgba(Status::Same), aa));
        }
    }
    let statuses = [
        Status::Modified,
        Status::Moved,
        Status::Added,
        Status::Removed,
        Status::Same,
    ];
    rows.extend(
        statuses
            .into_iter()
            .filter(|s| *s != Status::Same || show.unchanged)
            .map(|s| (status_rgba(s), status_name(s).to_owned())),
    );
    rows.extend(warnings.iter().map(|w| (warn, w.clone())));
    Legend { rows }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pdfxray_core::diff::{Confidence, PairKind, Sides};
    use pretty_assertions::assert_eq;

    use super::*;

    fn pair(status: Status, a: Option<u32>, b: Option<u32>) -> Pair {
        let node = |n| NodeId::StructElem(pdfxray_core::ObjRef::new(n, 0));
        let rect = Rect::new(10.0, 10.0, 50.0, 20.0);
        Pair {
            kind: PairKind::Element,
            status,
            a: a.map(node),
            b: b.map(node),
            role: format!("P{}", a.or(b).unwrap_or(0)),
            text: String::new(),
            page_a: a.map(|_| 0),
            page_b: b.map(|_| 0),
            bbox_a: a.map(|_| rect),
            bbox_b: b.map(|_| rect),
            delta: None,
            reordered: false,
            changed: Vec::new(),
            rows: Vec::new(),
            confidence: Confidence {
                rung: None,
                method: None,
                unicode_a: None,
                unicode_b: None,
                degraded_a: false,
                degraded_b: false,
            },
        }
    }

    fn pairing(pairs: Vec<Pair>) -> Pairing {
        let side = || pdfxray_core::diff::SideInfo {
            name: String::new(),
            pages: 1,
            elements: 0,
            structure_only: false,
            degraded_pages: Vec::new(),
        };
        Pairing {
            pairs,
            pages: Vec::new(),
            offset: None,
            tolerance: pdfxray_core::diff::TOLERANCE,
            sides: Sides { a: side(), b: side() },
            notes: Vec::new(),
            diagnostics: pdfxray_core::diff::DiagnosticsDiff::default(),
        }
    }

    fn summary(scene: &Scene) -> Vec<([u8; 4], Look, Option<&str>)> {
        scene
            .marks
            .iter()
            .map(|m| (m.color, m.look, m.chip.as_ref().map(|c| c.text.as_str())))
            .collect()
    }

    fn none<'a>(_: Side, _: &NodeId) -> Option<&'a TagBox> {
        None
    }

    fn tag(class: RoleClass, standard: &str) -> TagBox {
        TagBox {
            id: NodeId::Page(0),
            role: standard.to_owned(),
            standard: standard.to_owned(),
            class,
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            depth: 0,
        }
    }

    const LABELS: ChangeShow = ChangeShow {
        unchanged: false,
        labels: true,
        nesting: false,
    };

    fn all() -> Pairing {
        pairing(vec![
            pair(Status::Same, Some(1), Some(11)),
            pair(Status::Modified, Some(2), Some(12)),
            pair(Status::Moved, Some(3), Some(13)),
            pair(Status::Added, None, Some(14)),
            pair(Status::Removed, Some(5), None),
        ])
    }

    #[test]
    fn side_by_side_each_side_outlines_its_own_changes_in_status_colours_with_role_chips() {
        let [a, b] = change_scenes(&all(), (Some(0), Some(0)), Mode::SideBySide, LABELS, &none);

        let solid = Look::Solid;
        assert_eq!(
            summary(&a),
            [
                (status_rgba(Status::Modified), solid, Some("P2")),
                (status_rgba(Status::Moved), solid, Some("P3")),
                (status_rgba(Status::Removed), solid, Some("P5")),
            ]
        );
        assert_eq!(
            summary(&b),
            [
                (status_rgba(Status::Modified), solid, Some("P2")),
                (status_rgba(Status::Moved), solid, Some("P3")),
                (status_rgba(Status::Added), solid, Some("P14")),
            ]
        );
        // Smoke: an unchanged pair is not drawn by default.
        assert!(
            a.marks
                .iter()
                .chain(&b.marks)
                .all(|m| m.color != status_rgba(Status::Same))
        );
    }

    #[test]
    fn overlaid_an_element_is_outlined_once_and_a_move_leaves_a_faint_box_without_a_chip() {
        let show = ChangeShow {
            unchanged: true,
            ..LABELS
        };

        let [a, b] = change_scenes(&all(), (Some(0), Some(0)), Mode::Overlay, show, &none);

        assert_eq!(
            summary(&a),
            [
                (status_rgba(Status::Moved), Look::Container, None),
                (status_rgba(Status::Removed), Look::Solid, Some("P5")),
            ]
        );
        assert_eq!(b.marks.len(), 4, "same, modified, moved and added are B's");
        assert_eq!(a.marks[0].strength, LEFT_BEHIND);
        // Smoke: `--tags` brings the unchanged pair in, in the `same` colour.
        assert_eq!(b.marks[0].color, status_rgba(Status::Same));
    }

    #[test]
    fn containers_only_under_nesting_and_elements_of_another_page_never() {
        let sect = tag(RoleClass::Group, "Sect");
        let group = |_: Side, _: &NodeId| Some(&sect);
        let nesting = ChangeShow {
            nesting: true,
            ..LABELS
        };

        let hidden = change_scenes(&all(), (Some(0), Some(0)), Mode::SideBySide, LABELS, &group);
        let shown = change_scenes(&all(), (Some(0), Some(0)), Mode::SideBySide, nesting, &group);
        let elsewhere = change_scenes(&all(), (Some(1), Some(1)), Mode::SideBySide, LABELS, &none);

        assert_eq!(hidden.iter().map(|s| s.marks.len()).sum::<usize>(), 0);
        assert_eq!(shown[0].marks.len(), 3);
        assert_eq!(
            shown[0].marks[0]
                .chip
                .as_ref()
                .map(|c| (c.corner, c.text.as_str())),
            Some((Corner::TopRight, "Sect"))
        );
        // Smoke: nothing of page 0 lands on page 1.
        assert_eq!(elsewhere.iter().map(|s| s.marks.len()).sum::<usize>(), 0);
    }

    #[test]
    fn each_side_names_its_own_element_so_a_retag_reads_on_the_chips() {
        let (h2, h3) = (tag(RoleClass::Heading, "H2"), tag(RoleClass::Heading, "H3"));
        let by_side = |side: Side, _: &NodeId| Some(if side == Side::A { &h2 } else { &h3 });
        let retagged = pairing(vec![pair(Status::Modified, Some(2), Some(12))]);

        let [a, b] = change_scenes(&retagged, (Some(0), Some(0)), Mode::SideBySide, LABELS, &by_side);

        let chip = |s: &Scene| s.marks[0].chip.as_ref().map(|c| c.text.clone());
        assert_eq!(
            (chip(&a), chip(&b)),
            (Some("H2".to_owned()), Some("H3".to_owned()))
        );
        // Smoke: both boxes take the status colour, not a role colour.
        assert!(
            a.marks
                .iter()
                .chain(&b.marks)
                .all(|m| m.color == status_rgba(Status::Modified))
        );
    }

    #[test]
    fn the_legend_names_the_statuses_drawn_the_overlay_inks_and_the_warnings() {
        let warn = [1, 2, 3, 255];

        let overlay = change_legend(
            Mode::Overlay,
            LABELS,
            None,
            &["B page 0 is degraded".to_owned()],
            warn,
        );
        let side = change_legend(Mode::SideBySide, ChangeShow::default(), None, &[], warn);

        let names = |l: &Legend| l.rows.iter().map(|r| r.1.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(&overlay),
            [
                "ink of A",
                "ink of B",
                "modified",
                "moved",
                "added",
                "removed",
                "B page 0 is degraded"
            ]
        );
        assert_eq!(overlay.rows.last().map(|r| r.0), Some(warn));
        // Smoke: side by side there is no ink colour to explain.
        assert_eq!(names(&side), ["modified", "moved", "added", "removed"]);
    }

    #[test]
    fn a_mask_legend_names_both_ink_colours_and_states_the_score() {
        let score = VisualScore {
            width: 100,
            height: 50,
            differing_pixels: 21,
            ink_added: 20,
            ink_removed: 1,
            anti_aliased: 7,
            threshold: 0.1,
            include_aa: false,
            allowed: None,
        };

        let mask = change_legend(Mode::Mask, ChangeShow::default(), Some(&score), &[], [0; 4]);
        let overlay = change_legend(Mode::Overlay, ChangeShow::default(), Some(&score), &[], [0; 4]);

        let gray = status_rgba(Status::Same);
        assert_eq!(
            mask.rows[..4],
            [
                (INK_ADDED, "ink B adds".to_owned()),
                (INK_REMOVED, "ink B removes".to_owned()),
                (gray, "visual: 21 px differ (0.42%)".to_owned()),
                (gray, "7 px anti-aliasing, ignored".to_owned()),
            ]
        );
        // Smoke: only a mask has ink colours of its own to explain (they are the added and
        // removed status colours, so a box and a pixel of new ink read alike).
        assert!(
            overlay
                .rows
                .iter()
                .all(|r| !r.1.starts_with("ink B") && !r.1.starts_with("visual"))
        );
        assert_eq!(INK_ADDED, status_rgba(Status::Added));
    }
}
