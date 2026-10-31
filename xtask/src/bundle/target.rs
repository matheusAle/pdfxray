//! Which CPU architectures a bundle is built for.

use anyhow::{Result, bail};
use clap::ValueEnum;

/// A macOS CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Arch {
    /// Apple Silicon.
    Aarch64,
    /// Intel.
    X86_64,
}

impl Arch {
    /// Rust target triple.
    pub const fn triple(self) -> &'static str {
        match self {
            Self::Aarch64 => "aarch64-apple-darwin",
            Self::X86_64 => "x86_64-apple-darwin",
        }
    }

    /// The architecture of the machine running xtask.
    pub fn host() -> Result<Self> {
        Self::from_consts(std::env::consts::OS, std::env::consts::ARCH)
    }

    fn from_consts(os: &str, arch: &str) -> Result<Self> {
        match (os, arch) {
            ("macos", "aarch64") => Ok(Self::Aarch64),
            ("macos", "x86_64") => Ok(Self::X86_64),
            _ => bail!("`bundle` builds macOS apps and must run on macOS (this host is {os}/{arch})"),
        }
    }
}

/// The `--target` argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TargetArg {
    /// Apple Silicon only.
    #[value(name = "aarch64-apple-darwin")]
    Aarch64,
    /// Intel only.
    #[value(name = "x86_64-apple-darwin")]
    X86_64,
    /// Both, merged with `lipo`.
    #[value(name = "universal")]
    Universal,
}

/// What to build: one architecture or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// One architecture.
    Single(Arch),
    /// Fat binaries holding both.
    Universal,
}

impl Target {
    /// Resolves `--target`; without it, the host architecture.
    pub fn select(arg: Option<TargetArg>) -> Result<Self> {
        Self::select_for(arg, Arch::host)
    }

    fn select_for(arg: Option<TargetArg>, host: impl FnOnce() -> Result<Arch>) -> Result<Self> {
        Ok(match arg {
            Some(TargetArg::Aarch64) => Self::Single(Arch::Aarch64),
            Some(TargetArg::X86_64) => Self::Single(Arch::X86_64),
            Some(TargetArg::Universal) => Self::Universal,
            None => Self::Single(host()?),
        })
    }

    /// Architectures that must be compiled, in a stable order.
    pub fn arches(self) -> Vec<Arch> {
        match self {
            Self::Single(arch) => vec![arch],
            Self::Universal => vec![Arch::Aarch64, Arch::X86_64],
        }
    }

    /// The `<arch>` part of the zip name.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Single(Arch::Aarch64) => "aarch64",
            Self::Single(Arch::X86_64) => "x86_64",
            Self::Universal => "universal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_targets_win_over_the_host() {
        let host = || Ok(Arch::Aarch64);
        assert_eq!(
            Target::select_for(Some(TargetArg::X86_64), host).expect("x86"),
            Target::Single(Arch::X86_64)
        );
        assert_eq!(
            Target::select_for(Some(TargetArg::Universal), host).expect("fat"),
            Target::Universal
        );
        assert_eq!(
            Target::select_for(None, host).expect("host"),
            Target::Single(Arch::Aarch64)
        );
    }

    #[test]
    fn universal_builds_both_arches_in_order() {
        assert_eq!(Target::Universal.arches(), [Arch::Aarch64, Arch::X86_64]);
        assert_eq!(Target::Single(Arch::X86_64).arches(), [Arch::X86_64]);
    }

    #[test]
    fn triples_and_labels() {
        assert_eq!(Arch::Aarch64.triple(), "aarch64-apple-darwin");
        assert_eq!(Arch::X86_64.triple(), "x86_64-apple-darwin");
        assert_eq!(Target::Universal.label(), "universal");
        assert_eq!(Target::Single(Arch::Aarch64).label(), "aarch64");
        assert_eq!(Target::Single(Arch::X86_64).label(), "x86_64");
    }

    #[test]
    fn non_macos_hosts_are_refused() {
        assert_eq!(Arch::from_consts("macos", "aarch64").expect("arm"), Arch::Aarch64);
        assert_eq!(Arch::from_consts("macos", "x86_64").expect("intel"), Arch::X86_64);
        let err = Arch::from_consts("linux", "x86_64").expect_err("linux");
        assert!(err.to_string().contains("must run on macOS"), "{err}");
        assert!(Arch::from_consts("macos", "riscv64").is_err());
    }

    #[test]
    fn target_names_on_the_command_line() {
        let names: Vec<_> = TargetArg::value_variants()
            .iter()
            .filter_map(ValueEnum::to_possible_value)
            .map(|v| v.get_name().to_owned())
            .collect();
        assert_eq!(
            names,
            ["aarch64-apple-darwin", "x86_64-apple-darwin", "universal"]
        );
    }
}
