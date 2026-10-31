//! Release version parsing: `1.2.3`, `v1.2.3`, `1.2.3-rc.1`.

use std::fmt;

use anyhow::{Result, bail};

/// A `MAJOR.MINOR.PATCH[-PRERELEASE]` version. Build metadata (`+...`) is accepted and dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Major component.
    pub major: u64,
    /// Minor component.
    pub minor: u64,
    /// Patch component.
    pub patch: u64,
    /// Pre-release identifiers after `-`, if any.
    pub pre: Option<String>,
}

impl Version {
    /// Parses a version, tolerating a leading `v` (as in git tags).
    pub fn parse(text: &str) -> Result<Self> {
        let raw = text.trim();
        let rest = raw.strip_prefix('v').unwrap_or(raw);
        let rest = rest.split_once('+').map_or(rest, |(head, _)| head);
        let (core, pre) = match rest.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (rest, None),
        };
        let mut numbers = core.split('.');
        let (Some(major), Some(minor), Some(patch), None) =
            (numbers.next(), numbers.next(), numbers.next(), numbers.next())
        else {
            bail!("version `{raw}` is not MAJOR.MINOR.PATCH");
        };
        let number = |part: &str| -> Result<u64> {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                bail!("version `{raw}` has a non-numeric component `{part}`");
            }
            Ok(part.parse()?)
        };
        let pre = match pre {
            Some(pre) if !valid_pre(pre) => {
                bail!("version `{raw}` has an invalid pre-release `{pre}`")
            }
            Some(pre) => Some(pre.to_owned()),
            None => None,
        };
        Ok(Self {
            major: number(major)?,
            minor: number(minor)?,
            patch: number(patch)?,
            pre,
        })
    }

    /// `MAJOR.MINOR.PATCH`, the only form macOS accepts for `CFBundleShortVersionString`.
    pub fn core(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// Whether the numeric parts equal `other`'s (pre-release ignored).
    pub fn same_core(&self, other: &Self) -> bool {
        (self.major, self.minor, self.patch) == (other.major, other.minor, other.patch)
    }

    /// `CFBundleVersion`: three integers that grow with every release, release candidates
    /// included, because Sparkle decides whether an update is newer by comparing them.
    ///
    /// `MAJOR.MINOR.PATCH` alone cannot order `1.2.0-rc.1` before `1.2.0`, so the last integer is
    /// `PATCH` followed by a three-digit stage: `1NN` for `alpha.NN`, `2NN` for `beta.NN`, `3NN` for
    /// `rc.NN` (NN from 1 to 99) and `999` for the release itself. `1.2.0-rc.1` is `1.2.301`,
    /// `1.2.0` is `1.2.999`, `1.2.1-beta.2` is `1.2.1202`. Any other pre-release is an error.
    pub fn bundle_version(&self) -> Result<String> {
        let stage = match self.pre.as_deref() {
            None => FINAL_STAGE,
            Some(pre) => {
                let parsed = pre.split_once('.').and_then(|(name, text)| {
                    let base = STAGES.iter().find(|(stage, _)| *stage == name)?.1;
                    let number: u64 = text.parse().ok().filter(|n| (1..=99).contains(n))?;
                    // `rc.01` or `rc.+1` would sort differently in other tools; only plain numbers
                    (number.to_string() == text).then_some(base + number)
                });
                let Some(stage) = parsed else {
                    bail!(
                        "pre-release `{pre}` of version {self} cannot be ordered for Sparkle; use \
                         alpha.N, beta.N or rc.N with N from 1 to 99"
                    );
                };
                stage
            }
        };
        let Some(last) = self.patch.checked_mul(1000).and_then(|p| p.checked_add(stage)) else {
            bail!("version {self} is too large for CFBundleVersion");
        };
        Ok(format!("{}.{}.{last}", self.major, self.minor))
    }
}

/// The stage digits of a release: larger than every pre-release of the same `MAJOR.MINOR.PATCH`.
const FINAL_STAGE: u64 = 999;

/// The pre-releases [`Version::bundle_version`] can order, and the base of their stage digits.
const STAGES: [(&str, u64); 3] = [("alpha", 100), ("beta", 200), ("rc", 300)];

/// Dot-separated identifiers of `[0-9A-Za-z-]`, none empty.
fn valid_pre(pre: &str) -> bool {
    pre.split('.')
        .all(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_tag_and_prerelease_forms() {
        let v = Version::parse("1.2.3").expect("plain");
        assert_eq!((v.major, v.minor, v.patch, v.pre.as_deref()), (1, 2, 3, None));
        assert_eq!(Version::parse("v0.10.0").expect("tag").to_string(), "0.10.0");
        let rc = Version::parse(" v1.0.0-rc.1+build.5 ").expect("pre + build");
        assert_eq!(rc.pre.as_deref(), Some("rc.1"));
        assert_eq!(rc.to_string(), "1.0.0-rc.1");
        assert_eq!(rc.core(), "1.0.0");
    }

    #[test]
    fn rejects_malformed_versions() {
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "a.b.c",
            "1.2.x",
            "1..3",
            "-1.2.3",
            "1.2.3-",
            "1.2.3-a..b",
            "1.2.3-é",
            "v",
            "vv1.2.3",
        ] {
            assert!(Version::parse(bad).is_err(), "`{bad}` should be rejected");
        }
    }

    #[test]
    fn same_core_ignores_the_prerelease() {
        let a = Version::parse("1.2.3-rc.1").expect("a");
        let b = Version::parse("1.2.3").expect("b");
        let c = Version::parse("1.2.4").expect("c");
        assert!(a.same_core(&b));
        assert!(!a.same_core(&c));
    }

    fn bundle(text: &str) -> String {
        Version::parse(text)
            .expect("version")
            .bundle_version()
            .expect("bundle version")
    }

    #[test]
    fn bundle_versions_follow_the_documented_scheme() {
        let cases: Vec<(&str, String)> = [
            "0.1.0-alpha.1",
            "0.1.0-beta.2",
            "0.1.0-rc.1",
            "0.1.0-rc.99",
            "0.1.0",
            "1.2.1-beta.2",
            "10.20.30",
        ]
        .into_iter()
        .map(|v| (v, bundle(v)))
        .collect();
        assert_eq!(
            cases,
            [
                ("0.1.0-alpha.1", "0.1.101".to_owned()),
                ("0.1.0-beta.2", "0.1.202".to_owned()),
                ("0.1.0-rc.1", "0.1.301".to_owned()),
                ("0.1.0-rc.99", "0.1.399".to_owned()),
                ("0.1.0", "0.1.999".to_owned()),
                ("1.2.1-beta.2", "1.2.1202".to_owned()),
                ("10.20.30", "10.20.30999".to_owned()),
            ]
        );
    }

    /// Sparkle's comparison of two all-numeric `CFBundleVersion`s: component by component, as
    /// numbers (`SUStandardVersionComparator`).
    fn sparkle_order(a: &str, b: &str) -> std::cmp::Ordering {
        let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().expect("number")).collect() };
        parts(a).cmp(&parts(b))
    }

    #[test]
    fn bundle_versions_grow_across_releases_and_release_candidates() {
        let releases = [
            "0.1.0-alpha.1",
            "0.1.0-alpha.12",
            "0.1.0-beta.1",
            "0.1.0-rc.1",
            "0.1.0-rc.2",
            "0.1.0-rc.10",
            "0.1.0",
            "0.1.1-rc.1",
            "0.1.1",
            "0.1.9",
            "0.1.10-rc.1",
            "0.1.10",
            "0.2.0-rc.1",
            "0.2.0",
            "1.0.0-alpha.1",
            "1.0.0",
            "1.10.0",
        ];
        for pair in releases.windows(2) {
            let (older, newer) = (bundle(pair[0]), bundle(pair[1]));
            assert_eq!(
                sparkle_order(&older, &newer),
                std::cmp::Ordering::Less,
                "{} ({older}) must be older than {} ({newer})",
                pair[0],
                pair[1]
            );
        }
        // smoke: every value is what macOS wants, one to three period-separated integers
        for release in releases {
            let value = bundle(release);
            assert_eq!(value.split('.').count(), 3, "{value}");
            assert!(value.split('.').all(|p| p.parse::<u64>().is_ok()), "{value}");
        }
    }

    #[test]
    fn unorderable_pre_releases_are_rejected() {
        for bad in [
            "1.0.0-nightly",
            "1.0.0-rc",
            "1.0.0-rc.0",
            "1.0.0-rc.100",
            "1.0.0-rc.01",
            "1.0.0-rc.1.2",
            "1.0.0-RC.1",
            "1.0.0-pre.1",
        ] {
            let err = Version::parse(bad)
                .expect("parses")
                .bundle_version()
                .expect_err(bad);
            assert!(
                err.to_string().contains("alpha.N, beta.N or rc.N"),
                "{bad}: {err}"
            );
        }
        let huge = Version::parse("1.0.18446744073709551615").expect("parses");
        assert!(huge.bundle_version().is_err());
    }

    #[test]
    fn workspace_version_is_parseable() {
        let v = Version::parse(env!("CARGO_PKG_VERSION")).expect("workspace version");
        assert_eq!(v.to_string(), env!("CARGO_PKG_VERSION"));
    }
}
