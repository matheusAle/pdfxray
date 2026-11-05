//! `CHANGELOG.md`: a release's section is its notes, as markdown on the GitHub release
//! (`release-notes`) and as HTML in the app's update dialog (`sparkle appcast`).
//!
//! A section starts at `## [<version>]` (anything may follow on the line, such as the date) and
//! runs to the next `## ` heading. A release candidate without a section of its own uses the
//! section of the version it leads to (`0.4.0-rc.1` → `## [0.4.0]`).

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::bundle::version::Version;

/// The changelog, from the workspace root.
pub const FILE: &str = "CHANGELOG.md";

/// Where the whole history is read, linked from the update dialog's *Version History* button.
pub const URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/CHANGELOG.md");

/// The notes of `version` in `changelog`: its section without the heading, trimmed.
///
/// # Errors
///
/// When there is no section for the version (or, for a release candidate, for the version it
/// leads to), or the section is empty.
pub fn section<'a>(changelog: &'a str, version: &Version) -> Result<&'a str> {
    let exact = version.to_string();
    let core = version.core();
    let body = find(changelog, &exact).or_else(|| find(changelog, &core));
    match body {
        Some(body) if !body.is_empty() => Ok(body),
        Some(_) => bail!("the `## [{core}]` section of {FILE} is empty: write the release's notes in it"),
        None => bail!(
            "{FILE} has no `## [{core}]` section: move the notes under `## [Unreleased]` into one \
             before releasing {exact}"
        ),
    }
}

/// The body of the `## [key]` section, if there is one.
fn find<'a>(changelog: &'a str, key: &str) -> Option<&'a str> {
    let heading = format!("## [{key}]");
    let mut offset = 0;
    let mut start = None;
    for line in changelog.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        if let Some(from) = start {
            if line.starts_with("## ") {
                return Some(changelog[from..at].trim());
            }
        } else if line.trim_end().starts_with(&heading) {
            start = Some(offset);
        }
    }
    start.map(|from| changelog[from..].trim())
}

/// Reads [`FILE`] under `root`, the workspace root.
///
/// # Errors
///
/// When the file cannot be read.
pub fn read(root: &Path) -> Result<String> {
    let path = root.join(FILE);
    std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))
}

/// `release-notes`: prints the notes of `version` from `changelog`, as markdown.
///
/// # Errors
///
/// When `version` does not parse or the changelog has no notes for it.
pub fn release_notes_cmd(changelog: &str, version: &str, out: &mut dyn std::io::Write) -> Result<()> {
    let notes = section(changelog, &Version::parse(version)?)?;
    writeln!(out, "{notes}")?;
    Ok(())
}

/// The update dialog's page for the notes `markdown`: their HTML in the system font, in the
/// light or dark appearance the dialog has.
pub fn html(markdown: &str) -> String {
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, pulldown_cmark::Parser::new(markdown));
    let mut page = String::from(concat!(
        "<style>",
        ":root{color-scheme:light dark}",
        "body{font:13px -apple-system,system-ui,sans-serif;line-height:1.45;margin:0 4px}",
        "h3{font-size:13px;margin:14px 0 4px}",
        "h3:first-child{margin-top:4px}",
        "ul{margin:4px 0;padding-left:18px}",
        "li{margin:3px 0}",
        "code{font:12px ui-monospace,Menlo,monospace}",
        "</style>\n",
    ));
    page.push_str(&body);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog

Intro.

## [Unreleased]

### Added

- Next.

## [0.4.0] - 2026-11-05

### Added

- **Compare** two PDFs with `pdfxray diff`.

## [0.3.0] - 2026-11-02

- Tables.

## [0.2.0] - 2026-11-02
";

    fn version(text: &str) -> Version {
        Version::parse(text).expect("a version")
    }

    #[test]
    fn a_section_runs_from_its_heading_to_the_next_one() {
        assert_eq!(
            section(LOG, &version("0.4.0")).expect("0.4.0"),
            "### Added\n\n- **Compare** two PDFs with `pdfxray diff`."
        );
        assert_eq!(section(LOG, &version("v0.3.0")).expect("0.3.0"), "- Tables.");
        // A release candidate reads the section of the version it leads to.
        assert_eq!(
            section(LOG, &version("0.4.0-rc.1")).expect("rc"),
            section(LOG, &version("0.4.0")).expect("0.4.0")
        );
        // Smoke: `## [0.3.0]` does not answer for 0.3.1, nor `## [0.4.0]` for 0.4.
        assert!(find(LOG, "0.3.1").is_none());
        assert!(find(LOG, "0.4").is_none());
    }

    #[test]
    fn a_release_without_notes_is_refused() {
        let missing = section(LOG, &version("0.5.0")).expect_err("no section");
        assert_eq!(
            missing.to_string(),
            "CHANGELOG.md has no `## [0.5.0]` section: move the notes under `## [Unreleased]` \
             into one before releasing 0.5.0"
        );
        // The last section is empty: it runs to the end of the file.
        let empty = section(LOG, &version("0.2.0")).expect_err("empty");
        assert_eq!(
            empty.to_string(),
            "the `## [0.2.0]` section of CHANGELOG.md is empty: write the release's notes in it"
        );
    }

    #[test]
    fn release_notes_prints_the_section_as_markdown() {
        let mut out = Vec::new();

        release_notes_cmd(LOG, "v0.3.0", &mut out).expect("notes");

        assert_eq!(String::from_utf8(out).expect("utf8"), "- Tables.\n");
        // Smoke: an unknown version prints nothing and fails.
        let mut none = Vec::new();
        assert!(release_notes_cmd(LOG, "0.9.0", &mut none).is_err());
        assert_eq!(none, Vec::<u8>::new());
    }

    #[test]
    fn the_notes_become_html_in_the_system_font_for_both_appearances() {
        let page = html("### Added\n\n- **Compare** `Vec<T>` & more.\n");

        assert!(
            page.starts_with("<style>:root{color-scheme:light dark}"),
            "{page}"
        );
        assert!(page.contains("-apple-system"), "{page}");
        assert!(
            page.ends_with(
                "<h3>Added</h3>\n<ul>\n<li><strong>Compare</strong> <code>Vec&lt;T&gt;</code> &amp; more.</li>\n</ul>\n"
            ),
            "{page}"
        );
    }

    #[test]
    fn the_workspace_version_has_notes_in_the_changelog() {
        // Bumping the version without writing its notes fails here, before a tag is pushed.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let changelog = read(&root).expect("CHANGELOG.md");
        let current = version(env!("CARGO_PKG_VERSION"));

        let notes = section(&changelog, &current).expect("notes for the workspace version");

        assert!(notes.lines().count() >= 1, "{notes}");
        // Smoke: the Unreleased section is there for what comes next.
        assert!(find(&changelog, "Unreleased").is_some());
    }
}
