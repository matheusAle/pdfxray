//! veraPDF integration for pdfxray.
//!
//! veraPDF is a separate, GPL/MPL licensed Java program. This crate never bundles or links it:
//! it finds an installation ([`discover`]) or downloads a SHA-256 pinned release ([`install`]),
//! runs the CLI as a subprocess ([`VeraPdf::validate`]) and turns the machine readable XML
//! report ([`parse_report`]) into a [`Report`] of [`Finding`]s whose veraPDF context path is
//! parsed into a [`Locator`] ([`parse_context`]) that `pdfxray-core` maps onto its node ids.
//!
//! The report types belong to `pdfxray-core` ([`pdfxray_core::validation`]): this crate is the
//! adapter that produces them, so the dependency points from here to the core and the core
//! carries none of the XML, HTTP or hashing dependencies used here.
//!
//! [`Report`]: pdfxray_core::validation::Report
//! [`Finding`]: pdfxray_core::validation::Finding
//! [`Locator`]: pdfxray_core::validation::Locator

mod context;
mod discovery;
mod install;
mod java;
mod process;
mod profile;
mod report;
mod verapdf;
mod zip;

#[cfg(test)]
mod scratch;

pub use context::parse_context;
pub use discovery::{Discovery, DiscoveryConfig, default_install_dir, discover};
pub use install::{Asset, FetchError, Fetcher, InstallError, Release, UreqFetcher, install, install_with};
pub use java::{JavaRuntime, MIN_JAVA_MAJOR, find_java, probe_java};
pub use profile::Profile;
pub use report::{ParseError, parse_report};
pub use verapdf::{RunError, RunOptions, VeraPdf};
pub use zip::ZipError;
