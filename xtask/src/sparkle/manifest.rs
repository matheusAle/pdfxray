//! The pinned Sparkle release: version, archive URL, hash and size, and what an install holds.
//!
//! Releases come from <https://github.com/sparkle-project/Sparkle/releases>. Bumping Sparkle means
//! changing [`PINNED`] as a whole (download the new `Sparkle-<version>.tar.xz`, then
//! `shasum -a 256` the archive and `Sparkle.framework/Versions/B/Sparkle` inside it); the tests
//! check that the fields agree with each other.

/// A Sparkle release archive and the hash of the framework binary inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    /// Sparkle version, also the install directory below `target/sparkle/`.
    pub version: &'static str,
    /// Download URL of the `.tar.xz`.
    pub url: &'static str,
    /// SHA-256 of the archive, lowercase hex.
    pub sha256: &'static str,
    /// Size of the archive in bytes.
    pub size: u64,
    /// SHA-256 of [`FRAMEWORK_BINARY`], lowercase hex; checked whenever the install is looked at.
    pub binary_sha256: &'static str,
    /// Size of [`FRAMEWORK_BINARY`] in bytes.
    pub binary_size: u64,
}

/// The release pdfxray embeds.
pub const PINNED: Pin = Pin {
    version: "2.10.1",
    url: "https://github.com/sparkle-project/Sparkle/releases/download/2.10.1/Sparkle-2.10.1.tar.xz",
    sha256: "1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27",
    size: 16_346_336,
    binary_sha256: "9c78b2046e7637efeb8ca505d63170bc1557651925dbbfd45ac236845732b05a",
    binary_size: 976_240,
};

/// The framework's main binary, relative to the archive root and to an install.
pub const FRAMEWORK_BINARY: &str = "Sparkle.framework/Versions/B/Sparkle";

/// The framework directory, relative to the archive root and to an install.
pub const FRAMEWORK: &str = "Sparkle.framework";

/// Sparkle's tool that signs archives (for checking xtask's signatures against Sparkle's own).
pub const SIGN_UPDATE: &str = "bin/sign_update";

/// Sparkle's key tool (keychain based; `cargo xtask sparkle keygen` does not need it).
pub const GENERATE_KEYS: &str = "bin/generate_keys";

/// What an install keeps from the archive; everything else is dropped.
pub const KEPT: [&str; 4] = [FRAMEWORK, SIGN_UPDATE, GENERATE_KEYS, "LICENSE"];

#[cfg(test)]
mod tests {
    use super::*;

    fn is_sha256(s: &str) -> bool {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    #[test]
    fn the_pin_is_well_formed_and_consistent() {
        let v = PINNED.version;
        assert_eq!(
            PINNED.url,
            format!("https://github.com/sparkle-project/Sparkle/releases/download/{v}/Sparkle-{v}.tar.xz")
        );
        assert!(is_sha256(PINNED.sha256) && is_sha256(PINNED.binary_sha256));
        assert_ne!(PINNED.sha256, PINNED.binary_sha256);
        const { assert!(PINNED.size > PINNED.binary_size && PINNED.binary_size > 0) };
        // smoke: a Sparkle 2 release (the `B` framework version and the EdDSA tooling)
        assert!(v.starts_with("2."), "{v}");
        assert!(FRAMEWORK_BINARY.starts_with(FRAMEWORK));
        assert_eq!(
            KEPT,
            [
                "Sparkle.framework",
                "bin/sign_update",
                "bin/generate_keys",
                "LICENSE"
            ]
        );
    }
}
