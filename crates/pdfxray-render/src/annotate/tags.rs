//! The tag overlay as a [`Scene`]: every structure element's box in its role colour, the chips
//! that name them, the artifact sequences, and the reading order as numbered badges joined by a
//! path. What the canvas shows, and what an export of it draws.

use pdfxray_core::{
    Document, HeaderScope, NodeId, OrderStep, PageIx, ReadingOrder, Rect, RoleClass, StepKind, TagBox,
};

use super::{Chip, Corner, Look, Mark, Scene, Step};
use crate::xray::role_rgba;

/// Strength of the box of a role out of focus.
pub const UNFOCUSED_BOX: f32 = 0.15;

/// Strength of the reading-order path where it runs through a role out of focus.
pub const UNFOCUSED_PATH: f32 = 0.3;

/// What the chip of an artifact sequence says.
pub const ARTIFACT_CHIP: &str = "Art";

/// What the tag overlay shows: the View menu's options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TagShow {
    /// Outline every element (*All tag boxes*).
    pub boxes: bool,
    /// Put a chip naming each element by its box (*Tag labels*).
    pub labels: bool,
    /// Containers too (*Nesting*).
    pub nesting: bool,
    /// The artifacts too: the artifact sequences and the PDF 2.0 `Artifact` element (*Artifacts*).
    pub artifacts: bool,
}

/// An artifact sequence of a page: its number in paint order and where it paints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArtifactBox {
    /// Its number among the artifact sequences of the page, as in the Artifacts tree.
    pub seq: u32,
    /// Where it paints, page space.
    pub rect: Rect,
}

/// The artifact sequences of `page` that paint something, numbered as the Artifacts tree does.
pub fn artifact_boxes(doc: &Document, page: PageIx) -> Vec<ArtifactBox> {
    let Some(captured) = doc.captured(page) else {
        return Vec::new();
    };
    captured
        .marked
        .iter()
        .filter_map(|seq| seq.artifact.as_ref().map(|info| seq.bbox.or(info.bbox)))
        .zip(0u32..)
        .filter_map(|(rect, seq)| Some(ArtifactBox { seq, rect: rect? }))
        .collect()
}

/// The text of the chip of a tag box: the standard type it resolves to (`H1` for a `Heading1`
/// the role map sends there), else the type as written.
pub fn chip_text(tag: &TagBox) -> &str {
    if tag.standard.is_empty() {
        &tag.role
    } else {
        &tag.standard
    }
}

/// The note on the segment into a backtrack: `↰ goes back up: Caption read before Figure`.
pub fn backtrack_note(previous: &OrderStep, step: &OrderStep) -> String {
    format!("↰ goes back up: {} read before {}", previous.role, step.role)
}

/// What the tag overlay of one page is made of, and how it is shown.
pub struct TagView<'a> {
    /// The page.
    pub page: PageIx,
    /// The structure elements with a box on the page.
    pub tags: &'a [TagBox],
    /// The artifact sequences of the page.
    pub artifacts: &'a [ArtifactBox],
    /// The order to number; `None` leaves the badges and the path out.
    pub order: Option<&'a ReadingOrder>,
    /// Which boxes and labels show.
    pub show: TagShow,
    /// The scope of a table header cell, by element: its chip carries it.
    pub scope_of: &'a dyn Fn(&NodeId) -> Option<&'a HeaderScope>,
    /// Whether an element of a class with a standard type is in focus: drawn at full strength,
    /// with its chip. Out of focus it stays a faint box without a chip.
    pub in_focus: &'a dyn Fn(RoleClass, &str) -> bool,
}

impl TagShow {
    /// True when these options show the box of `tag`: containers only under *Nesting*, the PDF
    /// 2.0 `Artifact` element only under *Artifacts*.
    pub fn shows(self, tag: &TagBox) -> bool {
        match tag.class {
            RoleClass::Group => self.nesting,
            RoleClass::Artifact => self.artifacts,
            _ => true,
        }
    }
}

/// The scene of the tag overlay of `view`.
///
/// Every shown element is a mark in its role colour, outlined while the boxes are on and with a
/// chip while the labels are on and its role is in focus; a container's chip goes above its box,
/// clear of the chips of what it holds. The artifact sequences follow under *Artifacts*. The
/// reading order numbers its steps; the segment into a backtrack carries a note, the one into a
/// detour is dashed, and a segment that touches a role out of focus is faint.
pub fn tag_scene(view: &TagView<'_>) -> Scene {
    let TagView {
        page,
        tags,
        artifacts,
        order,
        show,
        scope_of,
        in_focus,
    } = *view;
    let tagged = show.boxes || show.labels;
    let strength = |focused: bool| if focused { 1.0 } else { UNFOCUSED_BOX };
    let mut marks: Vec<Mark> = tags
        .iter()
        .filter(|tag| tagged && show.shows(tag))
        .map(|tag| {
            let focused = in_focus(tag.class, &tag.standard);
            let container = tag.class == RoleClass::Group;
            Mark {
                node: tag.id.clone(),
                rect: tag.rect,
                color: role_rgba(tag.class),
                look: if container { Look::Container } else { Look::Solid },
                outlined: show.boxes,
                strength: strength(focused),
                chip: (show.labels && focused).then(|| Chip {
                    text: chip_text(tag).to_owned(),
                    corner: if container {
                        Corner::TopRight
                    } else {
                        Corner::TopLeft
                    },
                    scope: scope_of(&tag.id).cloned(),
                }),
            }
        })
        .collect();
    let artifact_focus = in_focus(RoleClass::Artifact, "");
    marks.extend(
        artifacts
            .iter()
            .filter(|_| tagged && show.artifacts)
            .map(|art| Mark {
                node: NodeId::Artifact { page, seq: art.seq },
                rect: art.rect,
                color: role_rgba(RoleClass::Artifact),
                look: Look::Thin,
                outlined: show.boxes,
                strength: strength(artifact_focus),
                chip: (show.labels && artifact_focus).then(|| Chip {
                    text: ARTIFACT_CHIP.to_owned(),
                    corner: Corner::TopLeft,
                    scope: None,
                }),
            }),
    );
    // The focus of a step's element: a table part is known by its box.
    let focus_of = |step: &OrderStep| {
        tags.iter().find(|tag| tag.id == step.id).map_or_else(
            || in_focus(step.class, ""),
            |tag| in_focus(tag.class, &tag.standard),
        )
    };
    let path = order.map_or_else(Vec::new, |order| {
        let steps = &order.steps;
        steps
            .iter()
            .enumerate()
            .map(|(n, step)| {
                let before = n.checked_sub(1).and_then(|b| steps.get(b));
                let through_focus = before.is_none_or(|before| focus_of(before) && focus_of(step));
                Step {
                    node: step.id.clone(),
                    rect: step.rect,
                    label: step.index.to_string(),
                    backtrack: step.backtrack,
                    dashed: step.kind == StepKind::Detour,
                    note: before
                        .filter(|_| step.backtrack)
                        .map(|before| backtrack_note(before, step)),
                    strength: if through_focus { 1.0 } else { UNFOCUSED_PATH },
                }
            })
            .collect()
    });
    Scene {
        marks,
        path,
        tint: None,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn tag(role: &str, standard: &str, class: RoleClass) -> TagBox {
        TagBox {
            id: NodeId::Document,
            role: role.to_owned(),
            standard: standard.to_owned(),
            class,
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            depth: 1,
        }
    }

    fn view<'a>(
        tags: &'a [TagBox],
        show: TagShow,
        in_focus: &'a dyn Fn(RoleClass, &str) -> bool,
    ) -> TagView<'a> {
        TagView {
            page: 0,
            tags,
            artifacts: &[],
            order: None,
            show,
            scope_of: &|_| None,
            in_focus,
        }
    }

    #[test]
    fn a_chip_says_the_standard_type_else_the_role() {
        assert_eq!(chip_text(&tag("Heading1", "H1", RoleClass::Heading)), "H1");
        assert_eq!(chip_text(&tag("Mystery", "", RoleClass::Other)), "Mystery");
    }

    #[test]
    fn containers_show_under_nesting_and_artifact_elements_under_artifacts() {
        let tags = [
            tag("Sect", "Sect", RoleClass::Group),
            tag("Artifact", "Artifact", RoleClass::Artifact),
            tag("P", "P", RoleClass::Paragraph),
        ];
        let all = TagShow {
            boxes: true,
            labels: true,
            nesting: true,
            artifacts: true,
        };
        let bare = TagShow {
            nesting: false,
            artifacts: false,
            ..all
        };
        let everything = |_: RoleClass, _: &str| true;
        let chips = |show| {
            tag_scene(&view(&tags, show, &everything))
                .marks
                .iter()
                .filter_map(|m| m.chip.as_ref().map(|c| c.text.clone()))
                .collect::<Vec<_>>()
        };

        assert_eq!(chips(all), ["Sect", "Artifact", "P"]);
        assert_eq!(chips(bare), ["P"]);
        // Smoke: with neither boxes nor labels there is nothing.
        assert_eq!(chips(TagShow::default()), Vec::<String>::new());
    }

    #[test]
    fn a_role_out_of_focus_is_a_faint_box_without_a_chip() {
        let tags = [
            tag("P", "P", RoleClass::Paragraph),
            tag("Figure", "Figure", RoleClass::Figure),
        ];
        let show = TagShow {
            boxes: true,
            labels: true,
            ..TagShow::default()
        };
        let figures = |class: RoleClass, _: &str| class == RoleClass::Figure;

        let scene = tag_scene(&view(&tags, show, &figures));

        let summary: Vec<(f32, bool, Corner)> = scene
            .marks
            .iter()
            .map(|m| {
                (
                    m.strength,
                    m.chip.is_some(),
                    m.chip.as_ref().map_or(Corner::TopLeft, |c| c.corner),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (UNFOCUSED_BOX, false, Corner::TopLeft),
                (1.0, true, Corner::TopLeft)
            ]
        );
        // Smoke: both are outlined, in their role colours.
        assert_eq!(
            scene
                .marks
                .iter()
                .map(|m| (m.outlined, m.color))
                .collect::<Vec<_>>(),
            [
                (true, role_rgba(RoleClass::Paragraph)),
                (true, role_rgba(RoleClass::Figure))
            ]
        );
    }
}
