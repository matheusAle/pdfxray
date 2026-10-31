//! The Sparkle feed (`appcast.xml`): one item for the release being published.

use std::fmt::Write as _;

use crate::bundle::plist::{APP_NAME, MIN_MACOS, escape};

/// The release an appcast announces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item<'a> {
    /// `sparkle:version`: the `CFBundleVersion` of the release, which Sparkle compares.
    pub bundle_version: &'a str,
    /// `sparkle:shortVersionString`: the version people read (`0.2.0`, `0.2.0-rc.1`).
    pub display_version: &'a str,
    /// Public URL of the update archive (the DMG).
    pub url: &'a str,
    /// Size of the archive in bytes.
    pub length: u64,
    /// `sparkle:edSignature` of the archive.
    pub ed_signature: &'a str,
    /// Publication time, seconds since 1970 (UTC).
    pub published: u64,
}

/// The feed text for `item`.
pub fn render(item: &Item<'_>) -> String {
    let mut xml = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n",
        "<rss version=\"2.0\" xmlns:sparkle=\"http://www.andymatuschak.org/xml-namespaces/sparkle\">\n",
        "  <channel>\n",
    ));
    let _ = writeln!(xml, "    <title>{}</title>", escape(APP_NAME));
    let _ = writeln!(
        xml,
        "    <description>Updates of {}</description>",
        escape(APP_NAME)
    );
    xml.push_str("    <language>en</language>\n    <item>\n");
    let _ = writeln!(
        xml,
        "      <title>{} {}</title>",
        escape(APP_NAME),
        escape(item.display_version)
    );
    let _ = writeln!(xml, "      <pubDate>{}</pubDate>", rfc822(item.published));
    let _ = writeln!(
        xml,
        "      <sparkle:version>{}</sparkle:version>",
        escape(item.bundle_version)
    );
    let _ = writeln!(
        xml,
        "      <sparkle:shortVersionString>{}</sparkle:shortVersionString>",
        escape(item.display_version)
    );
    let _ = writeln!(
        xml,
        "      <sparkle:minimumSystemVersion>{MIN_MACOS}</sparkle:minimumSystemVersion>"
    );
    let _ = writeln!(
        xml,
        "      <enclosure url=\"{}\" length=\"{}\" type=\"application/octet-stream\" sparkle:edSignature=\"{}\"/>",
        escape(item.url),
        item.length,
        escape(item.ed_signature)
    );
    xml.push_str("    </item>\n  </channel>\n</rss>\n");
    xml
}

/// `secs` since 1970 as an RFC 822 date in UTC, as RSS wants it: `Sat, 31 Oct 2026 12:00:00 +0000`.
pub fn rfc822(secs: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{}, {day:02} {} {year} {:02}:{:02}:{:02} +0000",
        DAYS[usize::try_from(days % 7).unwrap_or(0)],
        MONTHS[usize::try_from(month - 1).unwrap_or(0)],
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The proleptic Gregorian `(year, month, day)` of `days` since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`, for non-negative days).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_exactly_this_feed() {
        let item = Item {
            bundle_version: "0.2.999",
            display_version: "0.2.0",
            url: "https://dl.pdfxray.app/releases/0.2.0/pdfxray-0.2.0-macos-universal.dmg?a=1&b=2",
            length: 52_428_800,
            ed_signature: "c2ln+/==",
            published: 1_793_448_000,
        };
        assert_eq!(
            render(&item),
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<rss version=\"2.0\" xmlns:sparkle=\"http://www.andymatuschak.org/xml-namespaces/sparkle\">
  <channel>
    <title>pdfxray</title>
    <description>Updates of pdfxray</description>
    <language>en</language>
    <item>
      <title>pdfxray 0.2.0</title>
      <pubDate>Sat, 31 Oct 2026 12:00:00 +0000</pubDate>
      <sparkle:version>0.2.999</sparkle:version>
      <sparkle:shortVersionString>0.2.0</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>11.0</sparkle:minimumSystemVersion>
      <enclosure url=\"https://dl.pdfxray.app/releases/0.2.0/pdfxray-0.2.0-macos-universal.dmg?a=1&amp;b=2\" length=\"52428800\" type=\"application/octet-stream\" sparkle:edSignature=\"c2ln+/==\"/>
    </item>
  </channel>
</rss>
"
        );
    }

    #[test]
    fn dates_are_rfc_822_in_utc() {
        assert_eq!(rfc822(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        assert_eq!(rfc822(951_782_400), "Tue, 29 Feb 2000 00:00:00 +0000");
        assert_eq!(rfc822(1_793_448_000), "Sat, 31 Oct 2026 12:00:00 +0000");
        assert_eq!(rfc822(4_133_980_799), "Fri, 31 Dec 2100 23:59:59 +0000");
        // smoke: today's date is in this century and has the RFC 822 shape
        let now = crate::sparkle::unix_now();
        let text = rfc822(now);
        assert!(text.ends_with(" +0000") && text.contains(" 20"), "{text}");
        assert_eq!(text.len(), "Sat, 31 Oct 2026 12:00:00 +0000".len());
    }
}
