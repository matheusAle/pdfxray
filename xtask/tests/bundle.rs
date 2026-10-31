//! Full `cargo xtask bundle` for the host architecture, ad-hoc signed.
//!
//! Slow (two release builds) and macOS only, so it is ignored by default:
//! `cargo test -p xtask --test bundle -- --ignored`.

// Integration-test helpers sit outside `#[test]` fns, where clippy.toml's allowance does not reach.
#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use xtask::bundle::target::Arch;
use xtask::paths;

fn run_ok(cmd: &mut Command) -> String {
    let out = cmd.output().expect("spawn");
    assert!(
        out.status.success(),
        "{cmd:?} failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn bundle_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| root.join("target"), PathBuf::from)
        .join("bundle")
}

#[test]
#[ignore = "builds the app and the CLI in release mode; run with --ignored"]
fn host_bundle_is_a_valid_ad_hoc_signed_app() {
    let root = paths::workspace_root().expect("root");
    let version = env!("CARGO_PKG_VERSION");
    let (arch, lipo_arch) = match Arch::host().expect("macOS host") {
        Arch::Aarch64 => ("aarch64", "arm64"),
        Arch::X86_64 => ("x86_64", "x86_64"),
    };

    let mut log = Vec::new();
    xtask::run(["xtask", "bundle"], &mut log).expect("bundle succeeds");
    let log = String::from_utf8(log).expect("utf8 log");
    assert!(log.contains("ad-hoc signed"), "{log}");

    // app layout
    let app = bundle_dir(&root).join("pdfxray.app");
    for rel in [
        "Contents/Info.plist",
        "Contents/PkgInfo",
        "Contents/MacOS/pdfxray-app",
        "Contents/Resources/pdfxray.icns",
        "Contents/Resources/LICENSE",
        "Contents/Resources/THIRD-PARTY.txt",
        "Contents/_CodeSignature/CodeResources",
    ] {
        assert!(app.join(rel).is_file(), "{rel} missing from the app");
    }
    run_ok(
        Command::new("plutil")
            .arg("-lint")
            .arg(app.join("Contents/Info.plist")),
    );
    let plist = fs::read_to_string(app.join("Contents/Info.plist")).expect("plist");
    assert!(plist.contains("<string>dev.pdfxray.app</string>"));
    assert!(plist.contains(&format!("<string>{version}</string>")));
    // PDF is a document type the app views; plutil reads it back as such
    let types = run_ok(
        Command::new("plutil")
            .args(["-extract", "CFBundleDocumentTypes.0.LSItemContentTypes.0", "raw"])
            .arg(app.join("Contents/Info.plist")),
    );
    assert_eq!(types.trim(), "com.adobe.pdf");

    // signature: valid, ad-hoc, hardened runtime
    run_ok(
        Command::new("codesign")
            .args(["--verify", "--strict", "--verbose=2"])
            .arg(&app),
    );
    let details = run_ok(Command::new("codesign").args(["-dv", "--verbose=4"]).arg(&app));
    assert!(details.contains("Signature=adhoc"), "{details}");
    assert!(details.contains("Identifier=dev.pdfxray.app"), "{details}");
    assert!(details.contains("(adhoc,runtime)"), "{details}");

    // the executable is for this machine only
    let archs = run_ok(
        Command::new("lipo")
            .arg("-archs")
            .arg(app.join("Contents/MacOS/pdfxray-app")),
    );
    assert_eq!(archs.trim(), lipo_arch);

    // the zip: unpack it and run the CLI from it
    let release = format!("pdfxray-{version}-macos-{arch}");
    let zip = bundle_dir(&root).join(format!("{release}.zip"));
    assert!(zip.is_file(), "{} missing", zip.display());
    let unpacked = std::env::temp_dir().join(format!("xtask-bundle-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&unpacked);
    fs::create_dir_all(&unpacked).expect("mkdir");
    run_ok(Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&unpacked));

    let top = unpacked.join(&release);
    for name in [
        "pdfxray.app",
        "pdfxray",
        "README.md",
        "LICENSE",
        "THIRD-PARTY.txt",
    ] {
        assert!(top.join(name).exists(), "{name} missing from the zip");
    }
    let third = fs::read_to_string(top.join("THIRD-PARTY.txt")).expect("notice");
    assert!(third.contains("Crates ("), "{third}");
    assert!(third.contains("hayro-syntax"), "no hayro crate in the notice");
    assert!(
        !third.contains("pdfxray-core"),
        "workspace crate leaked into the notice"
    );

    let cli = top.join("pdfxray");
    run_ok(Command::new("codesign").args(["--verify", "--strict"]).arg(&cli));
    run_ok(
        Command::new("codesign")
            .args(["--verify", "--strict"])
            .arg(top.join("pdfxray.app")),
    );
    let cli_details = run_ok(Command::new("codesign").arg("-dv").arg(&cli));
    assert!(
        cli_details.contains("Identifier=dev.pdfxray.cli"),
        "{cli_details}"
    );
    let reported = run_ok(Command::new(&cli).arg("--version"));
    assert!(
        reported.contains(version),
        "`pdfxray --version` printed {reported:?}"
    );

    let _ = fs::remove_dir_all(&unpacked);
}
