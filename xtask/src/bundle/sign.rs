//! Code signing (`codesign`) and notarization (`notarytool`, `stapler`).
//!
//! Argument lists are built by pure functions so they can be tested without a certificate.
//! Secrets never appear on a command line: the app-specific password is handed to `notarytool` as
//! `@env:APPLE_APP_PASSWORD`, an API key is passed as the path of its `.p8` file, and error
//! messages never repeat arguments (see [`super::cmd`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::cmd;

/// Environment variable with the Apple ID used for notarization.
pub const ENV_APPLE_ID: &str = "APPLE_ID";
/// Environment variable with the developer team ID.
pub const ENV_TEAM_ID: &str = "APPLE_TEAM_ID";
/// Environment variable with the app-specific password of the Apple ID.
pub const ENV_PASSWORD: &str = "APPLE_APP_PASSWORD";
/// Environment variable with the path of an App Store Connect API key (`AuthKey_<ID>.p8`).
pub const ENV_API_KEY: &str = "APPLE_API_KEY";
/// Environment variable with the ID of that API key.
pub const ENV_API_KEY_ID: &str = "APPLE_API_KEY_ID";
/// Environment variable with the issuer ID (a UUID) of that API key.
pub const ENV_API_ISSUER: &str = "APPLE_API_ISSUER";

/// Who signs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    /// Ad-hoc signature (`-`): runs on this Mac, no developer identity, not notarizable.
    AdHoc,
    /// A "Developer ID Application" identity: name, SHA-1 hash or `Name (TEAMID)`.
    Developer(String),
}

/// Arguments for `codesign` to sign `path` with the hardened runtime and `entitlements`.
///
/// `identifier` overrides the code-signing identifier, which for a bare binary would otherwise be
/// derived from the file name.
pub fn codesign_args(
    identity: &Identity,
    entitlements: &Path,
    identifier: Option<&str>,
    path: &Path,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["--force".into(), "--options".into(), "runtime".into()];
    match identity {
        Identity::AdHoc => args.extend(["--timestamp=none".into(), "--sign".into(), "-".into()]),
        Identity::Developer(name) => {
            args.extend(["--timestamp".into(), "--sign".into(), name.into()]);
        }
    }
    args.extend(["--entitlements".into(), entitlements.into()]);
    if let Some(id) = identifier {
        args.extend(["--identifier".into(), id.into()]);
    }
    args.push(path.into());
    args
}

/// Signs `path`.
pub fn codesign(
    identity: &Identity,
    entitlements: &Path,
    identifier: Option<&str>,
    path: &Path,
) -> Result<()> {
    cmd::run(Command::new("codesign").args(codesign_args(identity, entitlements, identifier, path)))
        .with_context(|| format!("cannot sign {}", path.display()))
}

/// Arguments for `codesign --verify` of `path`.
pub fn verify_args(path: &Path) -> Vec<OsString> {
    vec![
        "--verify".into(),
        "--strict".into(),
        "--verbose=2".into(),
        path.into(),
    ]
}

/// Arguments for `codesign` to sign the disk image `dmg`.
///
/// A disk image is not executable code: no hardened runtime and no entitlements, only the
/// identity and a secure timestamp. Ad-hoc disk images stay unsigned, so there is no ad-hoc
/// variant.
pub fn codesign_dmg_args(identity: &str, dmg: &Path) -> Vec<OsString> {
    vec![
        "--force".into(),
        "--timestamp".into(),
        "--sign".into(),
        identity.into(),
        dmg.into(),
    ]
}

/// Signs the disk image `dmg` with the Developer ID `identity`.
pub fn codesign_dmg(identity: &str, dmg: &Path) -> Result<()> {
    cmd::run(Command::new("codesign").args(codesign_dmg_args(identity, dmg)))
        .with_context(|| format!("cannot sign {}", dmg.display()))
}

/// Checks the signature of `path`.
pub fn verify(path: &Path) -> Result<()> {
    cmd::run(Command::new("codesign").args(verify_args(path)))
        .with_context(|| format!("signature of {} does not verify", path.display()))
}

/// How `notarytool` authenticates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotaryAuth {
    /// A profile stored with `xcrun notarytool store-credentials`.
    Profile(String),
    /// An App Store Connect API key: the `.p8` file, its ID and its issuer. What CI uses.
    ApiKey {
        /// Path of the `AuthKey_<ID>.p8` file.
        key: PathBuf,
        /// The key ID.
        key_id: String,
        /// The issuer ID.
        issuer: String,
    },
    /// Apple ID and team from the environment; the password is read from [`ENV_PASSWORD`].
    AppleId {
        /// The Apple ID (email).
        apple_id: String,
        /// The developer team ID.
        team_id: String,
    },
}

impl NotaryAuth {
    /// Picks, in order: the profile if given; the API key when any `APPLE_API_*` variable is
    /// set; the Apple ID variables otherwise. Values come from `get`.
    ///
    /// Errors name the missing variables only, never a value. An API key file that does not
    /// exist is an error here, before the long build.
    pub fn resolve(profile: Option<&str>, get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        if let Some(profile) = profile {
            return Ok(Self::Profile(profile.to_owned()));
        }
        let value = |name: &str| get(name).filter(|v| !v.trim().is_empty());
        let api = [ENV_API_KEY, ENV_API_KEY_ID, ENV_API_ISSUER].map(|name| (name, value(name)));
        if api.iter().any(|(_, v)| v.is_some()) {
            let [(_, Some(key)), (_, Some(key_id)), (_, Some(issuer))] = api else {
                bail!(
                    "--notarize with an API key needs {}",
                    missing(&api.each_ref().map(|(name, v)| (*name, v.is_none())))
                );
            };
            let key = PathBuf::from(key);
            if !key.is_file() {
                bail!("{ENV_API_KEY} does not name an existing `.p8` file");
            }
            return Ok(Self::ApiKey { key, key_id, issuer });
        }
        let (apple_id, team_id, password) = (value(ENV_APPLE_ID), value(ENV_TEAM_ID), value(ENV_PASSWORD));
        match (apple_id, team_id, password) {
            (Some(apple_id), Some(team_id), Some(_)) => Ok(Self::AppleId { apple_id, team_id }),
            (apple_id, team_id, password) => bail!(
                "--notarize needs notarization credentials: set {ENV_API_KEY}, {ENV_API_KEY_ID} and \
                 {ENV_API_ISSUER} (API key), or {} (Apple ID), or pass --keychain-profile <NAME>",
                missing(&[
                    (ENV_APPLE_ID, apple_id.is_none()),
                    (ENV_TEAM_ID, team_id.is_none()),
                    (ENV_PASSWORD, password.is_none()),
                ])
            ),
        }
    }

    /// Authentication arguments for `notarytool`.
    pub fn args(&self) -> Vec<OsString> {
        match self {
            Self::Profile(name) => vec!["--keychain-profile".into(), name.into()],
            Self::ApiKey { key, key_id, issuer } => vec![
                "--key".into(),
                key.into(),
                "--key-id".into(),
                key_id.into(),
                "--issuer".into(),
                issuer.into(),
            ],
            Self::AppleId { apple_id, team_id } => vec![
                "--apple-id".into(),
                apple_id.into(),
                "--team-id".into(),
                team_id.into(),
                "--password".into(),
                format!("@env:{ENV_PASSWORD}").into(),
            ],
        }
    }
}

/// The names flagged absent, comma-separated.
fn missing(names: &[(&str, bool)]) -> String {
    names
        .iter()
        .filter_map(|(name, absent)| absent.then_some(*name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Arguments for `xcrun notarytool submit --wait`.
pub fn submit_args(auth: &NotaryAuth, zip: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "notarytool".into(),
        "submit".into(),
        zip.into(),
        "--wait".into(),
        "--output-format".into(),
        "json".into(),
    ];
    args.extend(auth.args());
    args
}

/// Arguments for `xcrun notarytool log`.
pub fn log_args(auth: &NotaryAuth, id: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["notarytool".into(), "log".into(), id.into()];
    args.extend(auth.args());
    args
}

/// The fields of the `notarytool submit --output-format json` result that matter.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Submission {
    /// Submission UUID.
    pub id: String,
    /// `Accepted`, `Invalid`, `Rejected`, ...
    pub status: String,
    /// Human-readable status message.
    #[serde(default)]
    pub message: String,
}

impl Submission {
    /// Parses the JSON that `notarytool` printed.
    pub fn parse(json: &str) -> Result<Self> {
        serde_json::from_str(json.trim()).context("cannot parse the `notarytool submit` result")
    }

    /// Whether Apple accepted the submission.
    pub fn is_accepted(&self) -> bool {
        self.status == "Accepted"
    }
}

/// Submits `zip` to the notary service, waits for the verdict, and fails unless it is `Accepted`
/// (printing Apple's log for the submission if not).
pub fn notarize(auth: &NotaryAuth, zip: &Path) -> Result<Submission> {
    let stdout = cmd::output(Command::new("xcrun").args(submit_args(auth, zip)))
        .context("notarization submission failed")?;
    let submission = Submission::parse(&stdout)?;
    if !submission.is_accepted() {
        let log = cmd::output(Command::new("xcrun").args(log_args(auth, &submission.id)))
            .unwrap_or_else(|e| format!("(cannot fetch the log: {e:#})"));
        bail!(
            "notarization {} was {}: {}\n{}",
            submission.id,
            submission.status,
            submission.message,
            log.trim_end()
        );
    }
    Ok(submission)
}

/// Staples the notarization ticket to `path` (an app or a disk image) and validates it.
pub fn staple(path: &Path) -> Result<()> {
    cmd::run(Command::new("xcrun").args(["stapler", "staple"]).arg(path))
        .with_context(|| format!("cannot staple {}", path.display()))?;
    cmd::run(Command::new("xcrun").args(["stapler", "validate"]).arg(path))
        .with_context(|| format!("stapled ticket of {} does not validate", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn developer_id_signing_uses_hardened_runtime_and_a_timestamp() {
        let args = codesign_args(
            &Identity::Developer("Developer ID Application: Example (ABCDE12345)".into()),
            Path::new("/e.plist"),
            Some("dev.pdfxray.cli"),
            Path::new("/b/pdfxray"),
        );
        assert_eq!(
            strings(&args),
            [
                "--force",
                "--options",
                "runtime",
                "--timestamp",
                "--sign",
                "Developer ID Application: Example (ABCDE12345)",
                "--entitlements",
                "/e.plist",
                "--identifier",
                "dev.pdfxray.cli",
                "/b/pdfxray",
            ]
        );
    }

    #[test]
    fn ad_hoc_signing_has_no_timestamp_and_no_identifier_by_default() {
        let args = strings(&codesign_args(
            &Identity::AdHoc,
            Path::new("/e.plist"),
            None,
            Path::new("/b/pdfxray.app"),
        ));
        assert_eq!(
            args,
            [
                "--force",
                "--options",
                "runtime",
                "--timestamp=none",
                "--sign",
                "-",
                "--entitlements",
                "/e.plist",
                "/b/pdfxray.app",
            ]
        );
        assert!(!args.iter().any(|a| a == "--deep"));
    }

    #[test]
    fn disk_images_are_signed_with_a_timestamp_and_no_runtime() {
        assert_eq!(
            strings(&codesign_dmg_args(
                "Developer ID Application: Example (ABCDE12345)",
                Path::new("/b/pdfxray.dmg")
            )),
            [
                "--force",
                "--timestamp",
                "--sign",
                "Developer ID Application: Example (ABCDE12345)",
                "/b/pdfxray.dmg",
            ]
        );
    }

    #[test]
    fn verification_is_strict() {
        assert_eq!(
            strings(&verify_args(Path::new("/a.app"))),
            ["--verify", "--strict", "--verbose=2", "/a.app"]
        );
    }

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn credentials_come_from_the_three_variables() {
        let auth = NotaryAuth::resolve(
            None,
            env(&[
                (ENV_APPLE_ID, "me@example.com"),
                (ENV_TEAM_ID, "TEAM123"),
                (ENV_PASSWORD, "s3cret-pw"),
            ]),
        )
        .expect("credentials");
        assert_eq!(
            auth,
            NotaryAuth::AppleId {
                apple_id: "me@example.com".into(),
                team_id: "TEAM123".into()
            }
        );
        let args = strings(&auth.args());
        assert_eq!(
            args,
            [
                "--apple-id",
                "me@example.com",
                "--team-id",
                "TEAM123",
                "--password",
                "@env:APPLE_APP_PASSWORD"
            ]
        );
        // the secret itself is never part of the arguments or the debug output
        assert!(!args.join(" ").contains("s3cret-pw"));
        assert!(!format!("{auth:?}").contains("s3cret-pw"));
    }

    #[test]
    fn a_keychain_profile_needs_no_variables() {
        let auth = NotaryAuth::resolve(Some("pdfxray-notary"), env(&[])).expect("profile");
        assert_eq!(strings(&auth.args()), ["--keychain-profile", "pdfxray-notary"]);
    }

    #[test]
    fn missing_credentials_name_only_the_missing_variables() {
        let err = NotaryAuth::resolve(
            None,
            env(&[(ENV_APPLE_ID, "me@example.com"), (ENV_PASSWORD, "   ")]),
        )
        .expect_err("incomplete");
        let text = err.to_string();
        assert!(text.contains("APPLE_TEAM_ID, APPLE_APP_PASSWORD"), "{text}");
        assert!(!text.contains("APPLE_ID,"), "{text}");
        assert!(text.contains("--keychain-profile"), "{text}");
        assert!(!text.contains("me@example.com"), "{text}");

        let err = NotaryAuth::resolve(None, env(&[])).expect_err("nothing");
        let text = err.to_string();
        assert!(
            text.contains("APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD"),
            "{text}"
        );
        assert!(
            text.contains("APPLE_API_KEY, APPLE_API_KEY_ID and APPLE_API_ISSUER"),
            "{text}"
        );
    }

    #[test]
    fn an_api_key_wins_over_the_apple_id_and_passes_only_the_key_path() {
        let tmp = crate::test_util::TempDir::new("notary-api-key");
        let key = tmp.write(
            "AuthKey_ABC123.p8",
            b"-----BEGIN PRIVATE KEY-----\nsecret-key-body",
        );
        let key_text = key.to_string_lossy().into_owned();
        let auth = NotaryAuth::resolve(
            None,
            env(&[
                (ENV_API_KEY, &key_text),
                (ENV_API_KEY_ID, "ABC123"),
                (ENV_API_ISSUER, "69a6de7e-0000-47e3-e053-5b8c7c11a4d1"),
                (ENV_APPLE_ID, "me@example.com"),
                (ENV_TEAM_ID, "TEAM123"),
                (ENV_PASSWORD, "s3cret-pw"),
            ]),
        )
        .expect("api key");
        assert_eq!(
            auth,
            NotaryAuth::ApiKey {
                key: key.clone(),
                key_id: "ABC123".into(),
                issuer: "69a6de7e-0000-47e3-e053-5b8c7c11a4d1".into(),
            }
        );
        let args = strings(&auth.args());
        assert_eq!(
            args,
            [
                "--key",
                key_text.as_str(),
                "--key-id",
                "ABC123",
                "--issuer",
                "69a6de7e-0000-47e3-e053-5b8c7c11a4d1",
            ]
        );
        assert!(!args.join(" ").contains("secret-key-body"));
    }

    #[test]
    fn an_incomplete_or_missing_api_key_is_reported_by_name() {
        let err = NotaryAuth::resolve(None, env(&[(ENV_API_KEY_ID, "ABC123")])).expect_err("partial");
        let text = err.to_string();
        assert!(
            text.contains("API key needs APPLE_API_KEY, APPLE_API_ISSUER"),
            "{text}"
        );
        assert!(!text.contains("ABC123"), "{text}");

        let err = NotaryAuth::resolve(
            None,
            env(&[
                (ENV_API_KEY, "/definitely/not/here/AuthKey.p8"),
                (ENV_API_KEY_ID, "ABC123"),
                (ENV_API_ISSUER, "issuer"),
            ]),
        )
        .expect_err("no file");
        let text = err.to_string();
        assert!(text.contains("APPLE_API_KEY does not name an existing"), "{text}");
        assert!(!text.contains("/definitely"), "{text}");
    }

    #[test]
    fn submit_waits_and_asks_for_json() {
        let auth = NotaryAuth::Profile("p".into());
        assert_eq!(
            strings(&submit_args(&auth, Path::new("/z.zip"))),
            [
                "notarytool",
                "submit",
                "/z.zip",
                "--wait",
                "--output-format",
                "json",
                "--keychain-profile",
                "p"
            ]
        );
        assert_eq!(
            strings(&log_args(&auth, "abc-123")),
            ["notarytool", "log", "abc-123", "--keychain-profile", "p"]
        );
    }

    #[test]
    fn parses_notarytool_results() {
        let ok = Submission::parse(
            r#"{"id":"2efe2717-52ef-43a5-96dc-0797e4ca1041","message":"Successfully received submission info","path":"/z.zip","status":"Accepted"}"#,
        )
        .expect("accepted");
        assert!(ok.is_accepted());
        assert_eq!(ok.id, "2efe2717-52ef-43a5-96dc-0797e4ca1041");

        let bad = Submission::parse(r#"{"id":"x","status":"Invalid"}"#).expect("invalid");
        assert!(!bad.is_accepted());
        assert_eq!(bad.message, "");

        assert!(Submission::parse("not json").is_err());
        assert!(Submission::parse(r#"{"status":"Accepted"}"#).is_err());
    }
}
