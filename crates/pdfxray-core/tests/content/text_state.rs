//! The `text-state` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn text_state_props_match_the_generator_facts() {
    let doc = open("text-state");
    let facts = expected("text-state");
    assert!(facts.text_ops.len() >= 10);

    for fact in &facts.text_ops {
        let p = op_props(&doc, fact.op_index, None);
        let at = format!("{} at {}", fact.operator, fact.op_index);

        assert_eq!(value(&p, "Operator", "Operator"), fact.operator, "{at}");
        assert_eq!(value(&p, "Text state", "Font"), fact.font, "{at}");
        assert_eq!(value(&p, "Text state", "Font size"), num(fact.font_size), "{at}");
        assert_eq!(
            value(&p, "Text state", "Effective size"),
            num(fact.effective_size),
            "{at}"
        );
        assert_eq!(value(&p, "Text state", "Tc"), num(fact.char_spacing), "{at}");
        assert_eq!(value(&p, "Text state", "Tw"), num(fact.word_spacing), "{at}");
        assert_eq!(
            value(&p, "Text state", "Tz"),
            num(fact.horizontal_scaling),
            "{at}"
        );
        assert_eq!(value(&p, "Text state", "Ts"), num(fact.rise), "{at}");
        assert_eq!(value(&p, "Text state", "TL"), num(fact.leading), "{at}");
        assert!(
            value(&p, "Text state", "Tr").starts_with(&fact.render_mode.to_string()),
            "{at}"
        );
        assert_eq!(value(&p, "Text state", "Text"), fact.text, "{at}");
    }
    let shown: usize = facts.text_ops.iter().map(|t| t.text.len()).sum();
    let listed: usize = facts
        .text_ops
        .iter()
        .map(|t| {
            let p = op_props(&doc, t.op_index, None);
            p.groups
                .iter()
                .find(|g| g.name == "Painted glyphs")
                .map_or(0, |g| g.rows.iter().filter(|r| r.link.is_some()).count())
        })
        .sum();
    assert_eq!(listed, shown);
}

#[test]
fn text_state_render_modes_and_matrix_sizes_read_naturally() {
    let doc = open("text-state");

    let invisible = op_props(&doc, 52, None);
    let stroke = op_props(&doc, 46, None);
    let both = op_props(&doc, 49, None);
    let sized_by_tm = op_props(&doc, 30, None);
    let spaced = op_props(&doc, 11, None);

    assert_eq!(value(&invisible, "Operator", "Operands"), "(Gone)");
    assert_eq!(value(&invisible, "Text state", "Tr"), "3 (invisible)");
    assert_eq!(value(&stroke, "Text state", "Tr"), "1 (stroke)");
    assert_eq!(value(&both, "Text state", "Tr"), "2 (fill, then stroke)");
    assert_eq!(value(&sized_by_tm, "Text state", "Font size"), "1");
    assert_eq!(value(&sized_by_tm, "Text state", "Effective size"), "16");
    assert_eq!(value(&spaced, "Text state", "Tz"), "150");
    assert_eq!(value(&spaced, "Text state", "Tc"), "2");
    assert_eq!(value(&spaced, "Text state", "Tw"), "5");
    // Invisible text is still captured: its glyphs are listed.
    assert!(invisible.groups.iter().any(|g| g.name == "Painted glyphs"));
}
