//! The `form-xobject-nesting` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn form_scopes_chain_matrices_exactly() {
    let page = fixture_page("form-xobject-nesting", 0);

    assert_eq!(page.xobjects.len(), 2);
    let (a, b) = (&page.xobjects[0], &page.xobjects[1]);
    assert_eq!(
        (a.name.as_str(), a.obj, a.parent, a.op),
        ("FmA", Some(ObjRef::new(6, 0)), None, 7)
    );
    assert_eq!(a.matrix, [0.8, 0.0, 0.0, 0.8, 20.0, 20.0]);
    assert_eq!(a.bbox, Some([0.0, 0.0, 250.0, 250.0]));
    assert_matrix(a.ctm, [0.8, 0.0, 0.0, 0.8, 20.0, 20.0]);
    assert_rect(a.page_bbox.unwrap(), [20.0, 20.0, 220.0, 220.0]);
    assert_eq!(
        (b.name.as_str(), b.obj, b.parent, b.op),
        ("FmB", Some(ObjRef::new(7, 0)), Some(0), 15)
    );
    assert_eq!(b.matrix, [1.0, 0.0, 0.0, 1.0, 10.0, 10.0]);
    // Form B's CTM is its /Matrix applied inside Form A: 0.8 * 10 + 20 = 28.
    assert_matrix(b.ctm, [0.8, 0.0, 0.0, 0.8, 28.0, 28.0]);
    assert_rect(b.page_bbox.unwrap(), [28.0, 28.0, 108.0, 108.0]);
    assert_eq!((a.first_op, a.last_op), (Some(8), Some(26)));
    assert_eq!((b.first_op, b.last_op), (Some(16), Some(26)));
    // The scopes agree with the fixture facts.
    let facts = expected("form-xobject-nesting");
    assert_eq!(
        facts
            .forms
            .iter()
            .map(|f| (f.object, f.name.as_str()))
            .collect::<Vec<_>>(),
        vec![(6, "FmA"), (7, "FmB")]
    );
    assert_eq!(facts.forms[0].matrix, Some(a.matrix));
    assert_eq!(facts.forms[1].matrix, Some(b.matrix));
}

#[test]
fn form_operators_know_their_stream_chain() {
    let page = fixture_page("form-xobject-nesting", 0);
    let page_stream = StreamRef::PageContent { index: 0 };
    let in_a = StreamRef::FormXObject {
        obj: Some(ObjRef::new(6, 0)),
        name: "FmA".to_owned(),
        parent: Box::new(page_stream.clone()),
    };
    let in_b = StreamRef::FormXObject {
        obj: Some(ObjRef::new(7, 0)),
        name: "FmB".to_owned(),
        parent: Box::new(in_a.clone()),
    };

    let streams: Vec<_> = page.ops.iter().map(|o| o.stream.clone()).collect();

    let mut want = vec![page_stream; 8];
    want.extend(vec![in_a; 8]);
    want.extend(vec![in_b; 11]);
    assert_eq!(streams, want);
    // local_ix restarts in each stream; spans are relative to the form's own decoded bytes.
    assert_eq!(page.ops[8].local_ix, 0);
    assert_eq!(page.ops[16].local_ix, 0);
    assert_eq!(page.ops[8].span, Some((0, 20)));
    assert_eq!(
        page.ops.iter().map(|o| o.xobject).collect::<Vec<_>>()[7..17],
        [
            None,
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(1)
        ]
    );
    assert_eq!(page.ops.len(), 27);
}

#[test]
fn form_content_is_placed_with_both_matrices() {
    let page = fixture_page("form-xobject-nesting", 0);
    let all = glyphs(&page);

    assert_eq!(text(&all), "Page textForm AForm B");
    // Page text at (10, 280); Form A text at (10, 220) under 0.8x + 20; Form B at (5, 15) under 28.
    let firsts: Vec<_> = all.iter().filter(|g| g.index == 0).map(|g| g.origin).collect();
    assert_points(&firsts, &[(10.0, 280.0), (28.0, 196.0), (32.0, 40.0)]);
    let fill = fills(&page);
    assert_eq!(fill.len(), 1);
    assert_rect(fill[0].bbox, [28.0, 28.0, 108.0, 60.0]);
    // The form /BBox clips what is inside: B's own box, within A's.
    assert_rect(fill[0].clip.unwrap(), [28.0, 28.0, 108.0, 108.0]);
    assert_eq!(fill[0].xobject_stack, vec![0, 1]);
    assert_eq!(fill[0].gstate.fill.components, vec![0.8, 1.0, 0.8]);
    assert!(all[0].item.xobject_stack.is_empty() && all[0].item.clip.is_none());
    // Glyph sizes scale with the CTM: 12 pt in Form A is 9.6 pt on the page.
    let a_glyph = all.iter().find(|g| g.unicode == "F").unwrap();
    assert!(close(a_glyph.advance, 0.611 * 12.0 * 0.8));
}

#[test]
fn form_marked_content_nests_under_the_invoking_streams() {
    let page = fixture_page("form-xobject-nesting", 0);

    assert_eq!(
        page.marked.iter().map(seq_summary).collect::<Vec<_>>(),
        vec![
            ("P", Some(0), 0, Some(6), None),
            ("P", Some(0), 8, Some(14), None),
            ("P", Some(0), 16, Some(26), None)
        ]
    );
    let stream_of_each: Vec<_> = page
        .marked
        .iter()
        .map(|s| match &s.stream {
            StreamRef::FormXObject { name, .. } => name.as_str(),
            StreamRef::PageContent { .. } => "page",
        })
        .collect();
    assert_eq!(stream_of_each, ["page", "FmA", "FmB"]);
    // Items in Form B belong to B's sequence (index 2) and to its scope chain.
    let b_items: Vec<_> = page.items.iter().filter(|i| i.xobject_stack == [0, 1]).collect();
    assert_eq!(b_items.len(), 7);
    assert!(b_items.iter().all(|i| i.mc_stack == [2]));
    assert_consistent(&page);
}
