//! The `graphics-state` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn graphics_state_props_match_the_generator_facts() {
    let doc = open("graphics-state");
    let facts = expected("graphics-state");
    assert!(facts.paint_ops.len() >= 10);

    for fact in &facts.paint_ops {
        let p = op_props(&doc, fact.op_index, None);
        let at = format!("{} at {}", fact.operator, fact.op_index);

        assert_eq!(value(&p, "Operator", "Operator"), fact.operator, "{at}");
        assert_eq!(value(&p, "Graphics state", "Blend mode"), fact.blend_mode, "{at}");
        if let Some(fill) = &fact.fill {
            let comps: Vec<String> = fill.components.iter().map(|c| num(*c)).collect();
            assert_eq!(value(&p, "Graphics state", "Fill space"), fill.space, "{at}");
            assert_eq!(value(&p, "Graphics state", "Fill color"), comps.join(" "), "{at}");
        }
        if let Some(stroke) = &fact.stroke {
            let comps: Vec<String> = stroke.components.iter().map(|c| num(*c)).collect();
            assert_eq!(value(&p, "Graphics state", "Stroke space"), stroke.space, "{at}");
            assert_eq!(
                value(&p, "Graphics state", "Stroke color"),
                comps.join(" "),
                "{at}"
            );
        }
        if let Some(alpha) = fact.fill_alpha {
            assert_eq!(value(&p, "Graphics state", "Alpha fill"), num(alpha), "{at}");
        }
        if let Some(alpha) = fact.stroke_alpha {
            assert_eq!(value(&p, "Graphics state", "Alpha stroke"), num(alpha), "{at}");
        }
        if let Some(line) = &fact.line {
            assert_eq!(value(&p, "Graphics state", "Line width"), num(line.width), "{at}");
            assert!(
                value(&p, "Graphics state", "Line cap").starts_with(&line.cap.to_string()),
                "{at}"
            );
            assert!(
                value(&p, "Graphics state", "Line join").starts_with(&line.join.to_string()),
                "{at}"
            );
            let dash = if line.dash_array.is_empty() {
                "solid".to_owned()
            } else {
                let items: Vec<String> = line.dash_array.iter().map(|d| num(*d)).collect();
                format!("[{}] {}", items.join(" "), num(line.dash_phase))
            };
            assert_eq!(value(&p, "Graphics state", "Dash"), dash, "{at}");
        }
        let clip = fact.clip.map_or_else(
            || "none".to_owned(),
            |c| fmt_rect(Rect::new(c[0], c[1], c[2], c[3])),
        );
        assert_eq!(value(&p, "Graphics state", "Clip box"), clip, "{at}");
    }
}

#[test]
fn graphics_state_concrete_values() {
    let doc = open("graphics-state");

    let dashed = op_props(&doc, 12, None);
    let translucent = op_props(&doc, 49, None);
    let clipped = op_props(&doc, 62, None);
    let plain = op_props(&doc, 3, None);

    assert_eq!(value(&dashed, "Graphics state", "Dash"), "[6 3] 2");
    assert_eq!(value(&dashed, "Graphics state", "Line cap"), "1 (round)");
    assert_eq!(value(&dashed, "Graphics state", "Line join"), "1 (round)");
    assert_eq!(value(&dashed, "Graphics state", "Stroke color"), "0 0 1");
    assert_eq!(value(&translucent, "Graphics state", "Alpha fill"), "0.5");
    assert_eq!(value(&translucent, "Graphics state", "Blend mode"), "Multiply");
    assert!(link_of(&translucent, "Graphics state", "ExtGState").is_some());
    assert_eq!(value(&clipped, "Graphics state", "Clip box"), "[200 220 350 320]");
    assert_eq!(value(&plain, "Graphics state", "Clip box"), "none");
    assert_eq!(value(&plain, "Graphics state", "ExtGState"), "none");
    assert_eq!(value(&plain, "Graphics state", "Fill color"), "1 0 0");
    assert_eq!(value(&plain, "Geometry", "Bounding box"), "[20 20 100 80]");
    assert_eq!(value(&plain, "Geometry", "Paints"), "Fill");
    // Fill operators show no text group.
    assert!(plain.groups.iter().all(|g| g.name != "Text state"));
}

#[test]
fn graphics_state_fill_boxes_match_the_facts() {
    let doc = open("graphics-state");
    let facts = expected("graphics-state");

    let mut checked = 0;
    for fact in facts.paint_ops.iter().filter(|f| f.operator == "f") {
        let p = op_props(&doc, fact.op_index, None);
        let b = fact.bbox;

        assert_eq!(
            value(&p, "Geometry", "Bounding box"),
            fmt_rect(Rect::new(b[0], b[1], b[2], b[3])),
            "f at {}",
            fact.op_index
        );
        checked += 1;
    }
    assert!(checked >= 5, "{checked}");
}
