//! `pdfxray info` against every generated fixture, checked against its expected facts.

use assert_cmd::Command;
use pretty_assertions::assert_eq;

#[test]
fn info_matches_expected_header_for_every_fixture() {
    let dir = pdfxray_fixtures::fixtures_dir();
    let fixtures = pdfxray_fixtures::all();

    let mut checked = Vec::new();
    for fixture in fixtures {
        let built = fixture.build().unwrap();
        let path = dir.join(fixture.pdf_file_name());
        let encrypted = fixture.name == "encrypted-stub";

        let out = Command::new(env!("CARGO_BIN_EXE_pdfxray"))
            .arg("info")
            .arg(&path)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let json: serde_json::Value = serde_json::from_slice(&out).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "name": fixture.pdf_file_name(),
                "byte_len": built.pdf.len(),
                "header_version": built.expected.header_version,
                "header_offset": 0,
                "page_count": built.expected.page_count,
                "object_count": json["object_count"],
                "revision_count": built.expected.revision_count,
                "encrypted": encrypted,
                "engine_error": if encrypted { Some("encrypted") } else { None },
                "diagnostics": json["diagnostics"],
            }),
            "fixture {}",
            fixture.name
        );
        checked.push(fixture.name);
    }

    // Smoke: the catalog really was exercised, including a defective and an encrypted file.
    // (Diagnostics per fixture are asserted exactly in pdfxray-raw and pdfxray-core tests.)
    assert_eq!(checked.len(), fixtures.len());
    assert!(checked.contains(&"broken-xref-offsets"));
    assert!(checked.contains(&"encrypted-stub"));
}
