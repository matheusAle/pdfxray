//! The flags and the JSON of the visual score of two files drawn by one engine
//! ([`pdfxray_render::score`]), shared by `render --against` and `diff --visual`.

use anyhow::anyhow;
use clap::Args;
use pdfxray_render::score::{self, Limits, Perceptual, VisualScore};
use serde::Serialize;

/// How two files' pages are compared pixel by pixel.
#[derive(Debug, Clone, Copy, Default, Args)]
pub struct VisualArgs {
    /// Per-pixel sensitivity of the visual score, 0 to 1, as a share of the largest YIQ colour
    /// distance: lower counts subtler changes, 0 any change. Default 0.1.
    #[arg(long, value_parser = unit)]
    threshold: Option<f64>,
    /// Count the pixels that look like anti-aliasing (the soft edge of a shape a fraction of a
    /// pixel elsewhere) in the visual score too. Off by default.
    #[arg(long)]
    include_aa: bool,
    /// The visual score exceeds when more than this many pixels differ. With
    /// `--max-diff-ratio` too, the stricter wins; with neither, no limit applies and nothing
    /// exceeds — a score is reported either way.
    #[arg(long)]
    max_diff_pixels: Option<u64>,
    /// The visual score exceeds when more than this share of the pixels (0 to 1) differ.
    #[arg(long, value_parser = unit)]
    max_diff_ratio: Option<f64>,
}

/// A number from 0 to 1.
fn unit(s: &str) -> Result<f64, String> {
    let v: f64 = s.parse().map_err(|e| format!("{e}"))?;
    if (0.0..=1.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{v} is not between 0 and 1"))
    }
}

impl VisualArgs {
    /// Fails, naming the first visual flag given, when `allowed` is false: the flags mean
    /// nothing without a comparison of two files to apply to.
    pub(crate) fn only_with(&self, allowed: bool, needs: &str) -> anyhow::Result<()> {
        let given = [
            (self.threshold.is_some(), "--threshold"),
            (self.include_aa, "--include-aa"),
            (self.max_diff_pixels.is_some(), "--max-diff-pixels"),
            (self.max_diff_ratio.is_some(), "--max-diff-ratio"),
        ];
        match given.into_iter().find(|(set, _)| *set) {
            Some((_, flag)) if !allowed => Err(anyhow!("{flag} needs {needs}")),
            _ => Ok(()),
        }
    }

    /// The comparison the flags ask for.
    pub(crate) fn perceptual(&self) -> Perceptual {
        Perceptual {
            threshold: self.threshold.unwrap_or(score::THRESHOLD),
            include_aa: self.include_aa,
            limits: Limits {
                pixels: self.max_diff_pixels,
                ratio: self.max_diff_ratio,
            },
            ..Perceptual::default()
        }
    }

    /// How the score was measured, for the JSON.
    pub(crate) fn settings(&self, scale: f32) -> Settings {
        let how = self.perceptual();
        Settings {
            scale,
            threshold: how.threshold,
            include_aa: how.include_aa,
            max_diff_pixels: self.max_diff_pixels,
            max_diff_ratio: self.max_diff_ratio,
        }
    }
}

/// How visual scores were measured.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct Settings {
    /// Pixels per point both sides were drawn at.
    pub(crate) scale: f32,
    /// [`Perceptual::threshold`].
    pub(crate) threshold: f64,
    /// [`Perceptual::include_aa`].
    pub(crate) include_aa: bool,
    /// The count limit, if one was set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_diff_pixels: Option<u64>,
    /// The ratio limit, if one was set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_diff_ratio: Option<f64>,
}

/// One visual score as JSON.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct ScoreView {
    /// Pixels that differ: the score.
    pub(crate) differing_pixels: u64,
    /// Their share of the page, 0 to 1.
    pub(crate) ratio: f64,
    /// Of those, pixels B draws darker than A.
    pub(crate) ink_added: u64,
    /// Of those, pixels B draws lighter than A.
    pub(crate) ink_removed: u64,
    /// Pixels beyond the threshold that look like anti-aliasing, counted or not.
    pub(crate) anti_aliased: u64,
    /// True when more pixels differ than the stricter limit allows.
    pub(crate) exceeds: bool,
}

impl From<&VisualScore> for ScoreView {
    fn from(v: &VisualScore) -> Self {
        Self {
            differing_pixels: v.differing_pixels,
            ratio: v.ratio(),
            ink_added: v.ink_added,
            ink_removed: v.ink_removed,
            anti_aliased: v.anti_aliased,
            exceeds: v.exceeds(),
        }
    }
}
