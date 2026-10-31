//! `Info.plist` and the code-signing entitlements, generated as XML text.

use std::fmt::Write as _;

use anyhow::Result;

use super::version::Version;
use crate::sparkle::FEED_URL;
use crate::sparkle::keys::PUBLIC_ED_KEY;

/// `CFBundleIdentifier` of the app.
pub const APP_BUNDLE_ID: &str = "dev.pdfxray.app";

/// Code-signing identifier of the standalone command-line binary.
pub const CLI_SIGN_ID: &str = "dev.pdfxray.cli";

/// `CFBundleName` and `CFBundleDisplayName`.
pub const APP_NAME: &str = "pdfxray";

/// Executable inside `Contents/MacOS` (the Cargo binary name of `pdfxray-app`).
pub const APP_EXECUTABLE: &str = "pdfxray-app";

/// Icon file in `Contents/Resources`, without extension.
pub const ICON_NAME: &str = "pdfxray";

/// Oldest macOS that runs the app; also exported as `MACOSX_DEPLOYMENT_TARGET` for the build.
pub const MIN_MACOS: &str = "11.0";

const HEADER: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
    "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
    "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
    "<plist version=\"1.0\">\n",
    "<dict>\n",
);

const FOOTER: &str = "</dict>\n</plist>\n";

/// `CFBundleTypeName` of the PDF document type.
pub const PDF_TYPE_NAME: &str = "PDF document";

/// Uniform type identifier of PDF.
pub const PDF_UTI: &str = "com.adobe.pdf";

/// Seconds between Sparkle's automatic update checks (`SUScheduledCheckInterval`): a day.
pub const UPDATE_CHECK_INTERVAL: u64 = 86_400;

/// `Contents/Info.plist` for `version`.
///
/// The app declares that it can view PDFs (`CFBundleDocumentTypes`), so it is offered in Finder's
/// "Open With" menu and accepts PDFs dropped on its Dock icon. The app receives the files through
/// its `NSApplicationDelegate` (the `pdfxray-macos` crate). The rank is `Alternate`: pdfxray never
/// takes over as the default PDF viewer by itself; the user can make it the default in Finder.
/// Several copies may run at once (`LSMultipleInstancesProhibited` false): the app starts one
/// process per window.
///
/// `CFBundleVersion` is [`Version::bundle_version`], which grows across release candidates too;
/// the `SU*` keys configure Sparkle, the updater: the feed, the public key every update must be
/// signed with (checked before the archive is even opened), and a daily automatic check.
pub fn info_plist(version: &Version) -> Result<String> {
    let core = version.core();
    let bundle_version = version.bundle_version()?;
    let pdf_type = [
        ("CFBundleTypeName", Value::Str(PDF_TYPE_NAME)),
        ("CFBundleTypeRole", Value::Str("Viewer")),
        ("LSHandlerRank", Value::Str("Alternate")),
        ("LSItemContentTypes", Value::Array(&[Value::Str(PDF_UTI)])),
    ];
    let document_types = [Value::Dict(&pdf_type)];
    let entries: [(&str, Value<'_>); 23] = [
        ("CFBundleDevelopmentRegion", Value::Str("en")),
        ("CFBundleDisplayName", Value::Str(APP_NAME)),
        ("CFBundleDocumentTypes", Value::Array(&document_types)),
        ("CFBundleExecutable", Value::Str(APP_EXECUTABLE)),
        ("CFBundleIconFile", Value::Str(ICON_NAME)),
        ("CFBundleIdentifier", Value::Str(APP_BUNDLE_ID)),
        ("CFBundleInfoDictionaryVersion", Value::Str("6.0")),
        ("CFBundleName", Value::Str(APP_NAME)),
        ("CFBundlePackageType", Value::Str("APPL")),
        ("CFBundleShortVersionString", Value::Str(&core)),
        ("CFBundleVersion", Value::Str(&bundle_version)),
        (
            "LSApplicationCategoryType",
            Value::Str("public.app-category.developer-tools"),
        ),
        ("LSMinimumSystemVersion", Value::Str(MIN_MACOS)),
        ("LSMultipleInstancesProhibited", Value::Bool(false)),
        (
            "NSHumanReadableCopyright",
            Value::Str("Copyright \u{a9} 2026 pdfxray contributors. MIT License."),
        ),
        ("NSHighResolutionCapable", Value::Bool(true)),
        ("NSPrincipalClass", Value::Str("NSApplication")),
        ("NSSupportsAutomaticGraphicsSwitching", Value::Bool(true)),
        ("SUEnableAutomaticChecks", Value::Bool(true)),
        ("SUFeedURL", Value::Str(FEED_URL)),
        ("SUPublicEDKey", Value::Str(PUBLIC_ED_KEY)),
        ("SUScheduledCheckInterval", Value::Int(UPDATE_CHECK_INTERVAL)),
        ("SUVerifyUpdateBeforeExtraction", Value::Bool(true)),
    ];
    Ok(render(&entries))
}

/// Hardened-runtime entitlements for the app and the CLI.
///
/// Everything that weakens the hardened runtime is stated and switched off. In particular
/// library validation stays on, so a `PDFium` dynamic library that is not signed by the same team
/// (or by Apple) cannot be loaded into the signed binaries; see `docs/releasing.md`.
pub fn entitlements_plist() -> String {
    let entries: [(&str, Value<'_>); 4] = [
        (
            "com.apple.security.cs.allow-dyld-environment-variables",
            Value::Bool(false),
        ),
        ("com.apple.security.cs.allow-jit", Value::Bool(false)),
        (
            "com.apple.security.cs.allow-unsigned-executable-memory",
            Value::Bool(false),
        ),
        (
            "com.apple.security.cs.disable-library-validation",
            Value::Bool(false),
        ),
    ];
    render(&entries)
}

enum Value<'a> {
    Str(&'a str),
    Bool(bool),
    Int(u64),
    Array(&'a [Value<'a>]),
    Dict(&'a [(&'a str, Value<'a>)]),
}

fn render(entries: &[(&str, Value<'_>)]) -> String {
    let mut xml = String::from(HEADER);
    write_entries(&mut xml, entries, 1);
    xml.push_str(FOOTER);
    xml
}

/// The `<key>` and value of every entry, indented `depth` tabs.
fn write_entries(xml: &mut String, entries: &[(&str, Value<'_>)], depth: usize) {
    for (key, value) in entries {
        let _ = writeln!(xml, "{}<key>{}</key>", "\t".repeat(depth), escape(key));
        write_value(xml, value, depth);
    }
}

fn write_value(xml: &mut String, value: &Value<'_>, depth: usize) {
    let indent = "\t".repeat(depth);
    match value {
        Value::Str(text) => {
            let _ = writeln!(xml, "{indent}<string>{}</string>", escape(text));
        }
        Value::Bool(true) => {
            let _ = writeln!(xml, "{indent}<true/>");
        }
        Value::Bool(false) => {
            let _ = writeln!(xml, "{indent}<false/>");
        }
        Value::Int(number) => {
            let _ = writeln!(xml, "{indent}<integer>{number}</integer>");
        }
        Value::Array(items) => {
            let _ = writeln!(xml, "{indent}<array>");
            for item in *items {
                write_value(xml, item, depth + 1);
            }
            let _ = writeln!(xml, "{indent}</array>");
        }
        Value::Dict(entries) => {
            let _ = writeln!(xml, "{indent}<dict>");
            write_entries(xml, entries, depth + 1);
            let _ = writeln!(xml, "{indent}</dict>");
        }
    }
}

/// Escapes the five XML special characters.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_INFO: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
<dict>
\t<key>CFBundleDevelopmentRegion</key>
\t<string>en</string>
\t<key>CFBundleDisplayName</key>
\t<string>pdfxray</string>
\t<key>CFBundleDocumentTypes</key>
\t<array>
\t\t<dict>
\t\t\t<key>CFBundleTypeName</key>
\t\t\t<string>PDF document</string>
\t\t\t<key>CFBundleTypeRole</key>
\t\t\t<string>Viewer</string>
\t\t\t<key>LSHandlerRank</key>
\t\t\t<string>Alternate</string>
\t\t\t<key>LSItemContentTypes</key>
\t\t\t<array>
\t\t\t\t<string>com.adobe.pdf</string>
\t\t\t</array>
\t\t</dict>
\t</array>
\t<key>CFBundleExecutable</key>
\t<string>pdfxray-app</string>
\t<key>CFBundleIconFile</key>
\t<string>pdfxray</string>
\t<key>CFBundleIdentifier</key>
\t<string>dev.pdfxray.app</string>
\t<key>CFBundleInfoDictionaryVersion</key>
\t<string>6.0</string>
\t<key>CFBundleName</key>
\t<string>pdfxray</string>
\t<key>CFBundlePackageType</key>
\t<string>APPL</string>
\t<key>CFBundleShortVersionString</key>
\t<string>1.2.3</string>
\t<key>CFBundleVersion</key>
\t<string>1.2.3304</string>
\t<key>LSApplicationCategoryType</key>
\t<string>public.app-category.developer-tools</string>
\t<key>LSMinimumSystemVersion</key>
\t<string>11.0</string>
\t<key>LSMultipleInstancesProhibited</key>
\t<false/>
\t<key>NSHumanReadableCopyright</key>
\t<string>Copyright \u{a9} 2026 pdfxray contributors. MIT License.</string>
\t<key>NSHighResolutionCapable</key>
\t<true/>
\t<key>NSPrincipalClass</key>
\t<string>NSApplication</string>
\t<key>NSSupportsAutomaticGraphicsSwitching</key>
\t<true/>
\t<key>SUEnableAutomaticChecks</key>
\t<true/>
\t<key>SUFeedURL</key>
\t<string>https://dl.pdf-xray.app/appcast.xml</string>
\t<key>SUPublicEDKey</key>
\t<string>PUBLIC_KEY</string>
\t<key>SUScheduledCheckInterval</key>
\t<integer>86400</integer>
\t<key>SUVerifyUpdateBeforeExtraction</key>
\t<true/>
</dict>
</plist>
";

    #[test]
    fn info_plist_is_exactly_this_xml() {
        let version = Version::parse("v1.2.3-rc.4").expect("version");
        // the pre-release suffix never reaches the plist as text: macOS wants three integers, and
        // CFBundleVersion encodes it as the stage digits (rc.4 is 304) so Sparkle orders it
        assert_eq!(
            info_plist(&version).expect("plist"),
            EXPECTED_INFO.replace("PUBLIC_KEY", PUBLIC_ED_KEY)
        );
    }

    #[test]
    fn a_release_after_its_candidate_has_a_larger_bundle_version() {
        let bundle_version = |v: &str| {
            let xml = info_plist(&Version::parse(v).expect("version")).expect("plist");
            let after = xml
                .split_once("<key>CFBundleVersion</key>\n\t<string>")
                .expect("key")
                .1;
            after.split_once('<').expect("end").0.to_owned()
        };
        assert_eq!(
            [
                bundle_version("0.1.0-rc.1"),
                bundle_version("0.1.0"),
                bundle_version("0.1.1-rc.1")
            ],
            ["0.1.301", "0.1.999", "0.1.1301"]
        );
        // smoke: a pre-release Sparkle cannot order stops the bundle instead of shipping
        let err = info_plist(&Version::parse("0.1.0-nightly").expect("version")).expect_err("nightly");
        assert!(err.to_string().contains("rc.N"), "{err}");
    }

    #[test]
    fn info_plist_views_pdfs_without_claiming_the_default() {
        let version = Version::parse("0.1.0").expect("version");
        let xml = info_plist(&version).expect("plist");
        // one document type, for PDF, as a viewer that only ranks as an alternative
        assert_eq!(xml.matches("<key>CFBundleTypeName</key>").count(), 1);
        assert!(xml.contains(
            "<key>LSItemContentTypes</key>\n\t\t\t<array>\n\t\t\t\t<string>com.adobe.pdf</string>"
        ));
        assert!(xml.contains("<key>CFBundleTypeRole</key>\n\t\t\t<string>Viewer</string>"));
        assert!(xml.contains("<key>LSHandlerRank</key>\n\t\t\t<string>Alternate</string>"));
        assert!(!xml.contains("<string>Owner</string>") && !xml.contains("<string>Default</string>"));
        // several windows are several processes
        assert!(xml.contains("<key>LSMultipleInstancesProhibited</key>\n\t<false/>"));
        assert!(xml.contains("<key>NSSupportsAutomaticGraphicsSwitching</key>\n\t<true/>"));
        // smoke: still one well-formed plist
        assert_eq!(xml.matches("<plist").count(), 1);
    }

    #[test]
    fn nested_values_are_indented_and_balanced() {
        let inner = [(
            "k",
            Value::Array(&[Value::Str("a&b"), Value::Bool(false), Value::Int(7)]),
        )];
        let xml = render(&[("outer", Value::Dict(&inner))]);
        assert!(xml.contains(
            "\t<key>outer</key>\n\t<dict>\n\t\t<key>k</key>\n\t\t<array>\n\t\t\t<string>a&amp;b</string>\n\t\t\t<false/>\n\t\t\t<integer>7</integer>\n\t\t</array>\n\t</dict>\n"
        ));
        assert_eq!(xml.matches("<dict>").count(), xml.matches("</dict>").count());
    }

    #[test]
    fn entitlements_switch_every_runtime_exception_off() {
        let xml = entitlements_plist();
        assert_eq!(xml.matches("<false/>").count(), 4);
        assert_eq!(xml.matches("<true/>").count(), 0);
        assert!(xml.contains("<key>com.apple.security.cs.disable-library-validation</key>\n\t<false/>"));
        assert!(xml.starts_with("<?xml"));
        assert!(xml.ends_with("</dict>\n</plist>\n"));
    }

    #[test]
    fn escapes_xml_specials() {
        assert_eq!(escape("a&b<c>\"d'"), "a&amp;b&lt;c&gt;&quot;d&apos;");
        assert_eq!(escape("plain"), "plain");
    }
}
