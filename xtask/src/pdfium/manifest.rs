//! The pinned `PDFium` build: release, per-platform asset URLs, hashes and sizes.
//!
//! Builds come from <https://github.com/bblanchon/pdfium-binaries>. Bumping the release means
//! changing [`VERSION`], [`TAG`] and every [`Platform`] below together; the tests check that
//! they agree with each other.

use anyhow::{Result, bail};

/// `PDFium` version of the pinned release (`MAJOR.MINOR.BUILD.PATCH`), also the install directory.
pub const VERSION: &str = "158.0.8125.0";

/// Release tag of `bblanchon/pdfium-binaries` that carries [`VERSION`].
pub const TAG: &str = "chromium/8125";

/// A pinned archive for one operating system and CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    /// Short name used in messages, `<os>-<arch>` as in the release asset names.
    pub id: &'static str,
    /// `std::env::consts::OS` value this archive is for.
    pub os: &'static str,
    /// `std::env::consts::ARCH` value this archive is for.
    pub arch: &'static str,
    /// Download URL of the `.tgz`.
    pub url: &'static str,
    /// SHA-256 of the `.tgz`, lowercase hex.
    pub sha256: &'static str,
    /// Size of the `.tgz` in bytes.
    pub size: u64,
    /// The dynamic library inside the archive and inside an install, relative to its root.
    pub library: &'static str,
    /// SHA-256 of the library, lowercase hex.
    pub library_sha256: &'static str,
    /// Size of the library in bytes.
    pub library_size: u64,
}

/// Every platform pdfxray knows how to fetch `PDFium` for.
pub const PLATFORMS: &[Platform] = &[
    Platform {
        id: "mac-arm64",
        os: "macos",
        arch: "aarch64",
        url: "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8125/pdfium-mac-arm64.tgz",
        sha256: "4939bd935ac83ddc09a3a4f038bc43a19cea1660316a3667f26cb07a070e139d",
        size: 3_540_999,
        library: "lib/libpdfium.dylib",
        library_sha256: "a45c44645523968525c710462691dd974f4efb1013ce1c249919a0939cfb373a",
        library_size: 7_388_864,
    },
    Platform {
        id: "mac-x64",
        os: "macos",
        arch: "x86_64",
        url: "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8125/pdfium-mac-x64.tgz",
        sha256: "8a5193535b80c7215d8932f6647ae5097465b6dba2811602c21e120c2af112b0",
        size: 3_749_070,
        library: "lib/libpdfium.dylib",
        library_sha256: "a9f428a16f46bab89646c8724b348e01c3fe707e3126d9356026cd876a74aecb",
        library_size: 7_809_288,
    },
    Platform {
        id: "linux-x64",
        os: "linux",
        arch: "x86_64",
        url: "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8125/pdfium-linux-x64.tgz",
        sha256: "e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659",
        size: 3_828_800,
        library: "lib/libpdfium.so",
        library_sha256: "1c251efce460d67437a4c76d4be9033ae2f17bf5603f5bff1af10209b94ca723",
        library_size: 8_049_768,
    },
    Platform {
        id: "linux-arm64",
        os: "linux",
        arch: "aarch64",
        url: "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8125/pdfium-linux-arm64.tgz",
        sha256: "8b8818f2ec57d1b213d952ca6d7f0ffeca9d0110487d0e20ad83ba918bc92efa",
        size: 3_728_574,
        library: "lib/libpdfium.so",
        library_sha256: "8cdc437dce84a6493b9ec6c4f5ad9ff2c299e505acfd7b0ad38cb440805b2c70",
        library_size: 8_127_624,
    },
];

/// The pinned platform for an `os`/`arch` pair as named by `std::env::consts`.
pub fn select(os: &str, arch: &str) -> Option<&'static Platform> {
    PLATFORMS.iter().find(|p| p.os == os && p.arch == arch)
}

/// The pinned platform for an `os`/`arch` pair, or an error naming the platforms that exist.
pub fn for_target(os: &str, arch: &str) -> Result<&'static Platform> {
    let Some(platform) = select(os, arch) else {
        let known: Vec<&str> = PLATFORMS.iter().map(|p| p.id).collect();
        bail!(
            "no pinned PDFium build for {os}/{arch}; known platforms: {}",
            known.join(", ")
        );
    };
    Ok(platform)
}

/// The pinned platform for the machine running xtask.
pub fn host() -> Result<&'static Platform> {
    for_target(std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_sha256(s: &str) -> bool {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    #[test]
    fn selects_the_platform_for_each_supported_host() {
        let id = |os, arch| select(os, arch).map(|p| p.id);
        assert_eq!(id("macos", "aarch64"), Some("mac-arm64"));
        assert_eq!(id("macos", "x86_64"), Some("mac-x64"));
        assert_eq!(id("linux", "x86_64"), Some("linux-x64"));
        assert_eq!(id("linux", "aarch64"), Some("linux-arm64"));
        // smoke: unsupported combinations are None, not a near match
        assert_eq!(id("windows", "x86_64"), None);
        assert_eq!(id("macos", "riscv64"), None);
        assert_eq!(id("", ""), None);
    }

    #[test]
    fn host_is_pinned_on_the_machines_ci_and_developers_use() {
        if matches!(std::env::consts::OS, "macos" | "linux") && std::env::consts::ARCH != "riscv64" {
            let platform = host().expect("host platform");
            assert_eq!(platform.os, std::env::consts::OS);
            assert_eq!(platform.arch, std::env::consts::ARCH);
        }
    }

    #[test]
    fn unsupported_target_error_names_the_known_platforms() {
        let err = for_target("plan9", "mips").expect_err("unsupported");
        assert_eq!(
            err.to_string(),
            "no pinned PDFium build for plan9/mips; known platforms: mac-arm64, mac-x64, linux-x64, linux-arm64"
        );
        assert_eq!(for_target("linux", "x86_64").expect("supported").id, "linux-x64");
    }

    #[test]
    fn every_pin_is_well_formed() {
        let base = format!("https://github.com/bblanchon/pdfium-binaries/releases/download/{TAG}/pdfium-");
        for p in PLATFORMS {
            assert_eq!(p.url, format!("{base}{}.tgz", p.id), "{}", p.id);
            assert!(is_sha256(p.sha256), "{} archive hash", p.id);
            assert!(is_sha256(p.library_sha256), "{} library hash", p.id);
            assert!(p.size > 0 && p.library_size > p.size, "{} sizes", p.id);
            assert!(p.library.starts_with("lib/libpdfium."), "{} library", p.id);
            assert_ne!(p.sha256, p.library_sha256, "{}", p.id);
        }
        // smoke: ids and (os, arch) pairs are unique
        for (i, a) in PLATFORMS.iter().enumerate() {
            for b in &PLATFORMS[i + 1..] {
                assert_ne!(a.id, b.id);
                assert_ne!((a.os, a.arch), (b.os, b.arch));
            }
        }
    }

    #[test]
    fn tag_and_version_name_the_same_build() {
        let build = VERSION.split('.').nth(2).expect("BUILD part");
        assert_eq!(TAG, format!("chromium/{build}"));
        assert_eq!(VERSION.split('.').count(), 4);
    }

    #[test]
    fn libraries_use_the_extension_of_their_os() {
        for p in PLATFORMS {
            let expected = if p.os == "macos" { ".dylib" } else { ".so" };
            assert!(p.library.ends_with(expected), "{}: {}", p.id, p.library);
        }
    }
}
