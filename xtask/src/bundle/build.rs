//! Compiling the shipped binaries and merging architectures with `lipo`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::cmd;
use super::layout::CLI_BINARY;
use super::plist::{APP_EXECUTABLE, MIN_MACOS};
use super::target::{Arch, Target};

/// Cargo package and binary name of one shipped executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binary {
    /// Cargo package (`-p`).
    pub package: &'static str,
    /// Binary file name.
    pub name: &'static str,
}

/// The desktop app executable.
pub const APP: Binary = Binary {
    package: "pdfxray-app",
    name: APP_EXECUTABLE,
};
/// The command-line binary.
pub const CLI: Binary = Binary {
    package: "pdfxray-cli",
    name: CLI_BINARY,
};

/// Arguments of `cargo build` for `binary` on `arch`.
///
/// The two binaries are built by separate invocations on purpose: one invocation would unify
/// Cargo features, and the CLI's `pdfium` feature would leak into the app.
pub fn cargo_args(binary: Binary, arch: Arch) -> Vec<OsString> {
    [
        "build",
        "--release",
        "--locked",
        "--package",
        binary.package,
        "--target",
        arch.triple(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

/// Where `cargo build` leaves `binary` for `arch`.
pub fn built_path(target_dir: &Path, binary: Binary, arch: Arch) -> PathBuf {
    target_dir.join(arch.triple()).join("release").join(binary.name)
}

/// Arguments of `lipo` that merge `inputs` into `output`.
pub fn lipo_args(inputs: &[PathBuf], output: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["-create".into(), "-output".into(), output.into()];
    args.extend(inputs.iter().map(OsString::from));
    args
}

/// Builds `binary` for every architecture of `target` and returns the (possibly fat) result,
/// written to `out_dir/<name>`.
pub fn build(target_dir: &Path, out_dir: &Path, binary: Binary, target: Target) -> Result<PathBuf> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut built = Vec::new();
    for arch in target.arches() {
        cmd::run(
            Command::new(&cargo)
                .args(cargo_args(binary, arch))
                .env("MACOSX_DEPLOYMENT_TARGET", MIN_MACOS),
        )
        .with_context(|| {
            format!(
                "cannot build {} for {} (is the target installed? `rustup target add {}`)",
                binary.package,
                arch.triple(),
                arch.triple()
            )
        })?;
        let path = built_path(target_dir, binary, arch);
        if !path.is_file() {
            bail!("cargo did not produce {}", path.display());
        }
        built.push(path);
    }

    fs::create_dir_all(out_dir).with_context(|| format!("cannot create {}", out_dir.display()))?;
    let out = out_dir.join(binary.name);
    if out.exists() {
        fs::remove_file(&out).with_context(|| format!("cannot remove {}", out.display()))?;
    }
    match built.as_slice() {
        [only] => {
            fs::copy(only, &out).with_context(|| format!("cannot copy {}", only.display()))?;
        }
        many => cmd::run(Command::new("lipo").args(lipo_args(many, &out)))
            .with_context(|| format!("cannot merge {} with lipo", binary.name))?,
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn cargo_builds_one_package_per_invocation_for_a_target() {
        assert_eq!(
            strings(&cargo_args(CLI, Arch::X86_64)),
            [
                "build",
                "--release",
                "--locked",
                "--package",
                "pdfxray-cli",
                "--target",
                "x86_64-apple-darwin"
            ]
        );
        let app = strings(&cargo_args(APP, Arch::Aarch64));
        assert!(app.contains(&"pdfxray-app".to_owned()) && !app.contains(&"pdfxray-cli".to_owned()));
    }

    #[test]
    fn binary_names_match_the_cargo_manifests() {
        assert_eq!((APP.package, APP.name), ("pdfxray-app", "pdfxray-app"));
        assert_eq!((CLI.package, CLI.name), ("pdfxray-cli", "pdfxray"));
    }

    #[test]
    fn built_binaries_live_under_the_triple() {
        assert_eq!(
            built_path(Path::new("/t"), CLI, Arch::Aarch64),
            Path::new("/t/aarch64-apple-darwin/release/pdfxray")
        );
    }

    #[test]
    fn lipo_merges_all_inputs_into_the_output() {
        let args = lipo_args(&[PathBuf::from("/a"), PathBuf::from("/b")], Path::new("/out"));
        assert_eq!(strings(&args), ["-create", "-output", "/out", "/a", "/b"]);
    }
}
