//! Motion: the few named animations of the design, all interruptible, all switched off by
//! "Reduce motion".
//!
//! Booleans ride on egui's `animate_bool_with_time_and_easing`. Values are tweened here, from the
//! value currently shown to the new target, so a target that changes mid-flight retargets from
//! where the value is instead of jumping. [`appear`] is the "just opened" animation that popovers
//! and tooltips use.
//!
//! # Reduce motion
//!
//! [`reduce_motion`] is read once: the environment variable `PDFXRAY_REDUCE_MOTION` (`1` or `0`)
//! wins, otherwise macOS' `defaults read com.apple.universalaccess reduceMotion`, otherwise
//! `false`. Tests (and anything else that needs it) can force a value with
//! [`set_reduce_motion`]; a forced value is never overwritten by the startup read. While it is
//! on, every function here returns its end state at once.

use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};

use egui::{Context, Id};
use pdfxray_core::numeric::f32_from_f64;

/// An animation's length and curve.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Length in seconds.
    pub secs: f32,
    /// Curve from 0 to 1, see [`easing`].
    pub easing: fn(f32) -> f32,
}

/// Easing curves: each maps 0..=1 to 0..=1 and fixes both ends.
pub mod easing {
    /// No easing.
    pub const fn linear(t: f32) -> f32 {
        t
    }

    /// Fast start, slow end (cubic).
    pub fn ease_out(t: f32) -> f32 {
        1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
    }

    /// Slow start and end (cubic).
    pub fn ease_in_out(t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }
}

/// The selection outline grows in.
pub const SELECTION: Timing = Timing {
    secs: 0.120,
    easing: easing::ease_out,
};
/// A popover, menu or tooltip fades and scales in.
pub const POPOVER: Timing = Timing {
    secs: 0.140,
    easing: easing::ease_out,
};
/// Zoom and jump-to-node.
pub const ZOOM: Timing = Timing {
    secs: 0.220,
    easing: easing::ease_in_out,
};
/// A newly painted item fades in while stepping through the paint order.
pub const PAINT_STEP: Timing = Timing {
    secs: 0.090,
    easing: easing::ease_out,
};
/// The reading-order path draws from 1 to n.
pub const PATH: Timing = Timing {
    secs: 0.300,
    easing: easing::ease_in_out,
};
/// A value that changed flashes a surface.
pub const FLASH: Timing = Timing {
    secs: 0.400,
    easing: easing::ease_out,
};

/// Where a popover starts: scale 0.97, growing to 1.
pub const POPOVER_FROM_SCALE: f32 = 0.97;

const UNSET: u8 = 0;
const ON: u8 = 1;
const OFF: u8 = 2;
static REDUCE: AtomicU8 = AtomicU8::new(UNSET);

/// Whether animations are skipped. Resolved on the first call, see the module docs.
pub fn reduce_motion() -> bool {
    match REDUCE.load(Ordering::Relaxed) {
        ON => true,
        OFF => false,
        _ => {
            let on = resolve(
                std::env::var("PDFXRAY_REDUCE_MOTION").ok().as_deref(),
                system_setting().as_deref(),
            );
            set_reduce_motion(on);
            on
        }
    }
}

/// Forces the setting, e.g. for tests, overriding the environment and the system.
pub fn set_reduce_motion(on: bool) {
    REDUCE.store(if on { ON } else { OFF }, Ordering::Relaxed);
}

/// The setting from the environment value (`1` or `0`, else ignored) and macOS' answer (`1` is
/// on, anything else is off).
fn resolve(env: Option<&str>, system: Option<&str>) -> bool {
    match env.map(str::trim) {
        Some("1") => true,
        Some("0") => false,
        _ => system.is_some_and(|out| out.trim() == "1"),
    }
}

/// The output of macOS' `reduceMotion` default. Any failure (another OS, no such key, no `defaults`)
/// is `None`.
fn system_setting() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let out = Command::new("defaults")
        .args(["read", "com.apple.universalaccess", "reduceMotion"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Eases `id` towards 1.0 while `on` and towards 0.0 while not. The first call returns the end
/// state without animating. Interruptible: it turns around from where it is.
pub fn flag(ctx: &Context, id: Id, on: bool, timing: Timing) -> f32 {
    if reduce_motion() {
        return if on { 1.0 } else { 0.0 };
    }
    ctx.animate_bool_with_time_and_easing(id, on, timing.secs, timing.easing)
}

/// A tween from `from` to `to` that began at `start` (seconds of egui's clock).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tween {
    from: f32,
    to: f32,
    start: f64,
}

impl Tween {
    /// How far (0..=1, before easing) the tween is at `now`.
    fn progress(self, now: f64, timing: Timing) -> f32 {
        if timing.secs <= 0.0 {
            return 1.0;
        }
        // Frame times are tiny numbers; f32 is enough for a fraction.
        let elapsed = f32_from_f64(now - self.start);
        (elapsed / timing.secs).clamp(0.0, 1.0)
    }

    fn at(self, now: f64, timing: Timing) -> f32 {
        let t = (timing.easing)(self.progress(now, timing));
        self.from + (self.to - self.from) * t
    }
}

fn run(ctx: &Context, id: Id, target: f32, timing: Timing, first: Option<f32>) -> f32 {
    let now = ctx.input(|i| i.time);
    let key = id.with("pdfxray-motion");
    let stored: Option<Tween> = ctx.data(|d| d.get_temp(key));
    let mut tween = stored.unwrap_or(Tween {
        from: first.unwrap_or(target),
        to: target,
        start: now,
    });
    if tween.to.total_cmp(&target).is_ne() {
        // Interrupted: carry on from the value being shown.
        tween = Tween {
            from: tween.at(now, timing),
            to: target,
            start: now,
        };
    }
    ctx.data_mut(|d| d.insert_temp(key, tween));
    if tween.progress(now, timing) < 1.0 {
        ctx.request_repaint();
    }
    tween.at(now, timing)
}

/// The value of `id` on its way to `target`, linear in value and eased in time. The first call
/// returns `target`; a later change of `target` tweens from the value shown. Zoom is the typical
/// use: `value(ctx, id, zoom, ZOOM)`.
pub fn value(ctx: &Context, id: Id, target: f32, timing: Timing) -> f32 {
    if reduce_motion() {
        ctx.data_mut(|d| d.remove::<Tween>(id.with("pdfxray-motion")));
        return target;
    }
    run(ctx, id, target, timing, None)
}

/// 0.0 on the first call for `id`, then 1.0 once `timing` has passed: the "just appeared"
/// animation. Call it every frame the thing is shown and [`reset`] it when it is hidden, so it
/// runs again next time.
pub fn appear(ctx: &Context, id: Id, timing: Timing) -> f32 {
    if reduce_motion() {
        return 1.0;
    }
    run(ctx, id, 1.0, timing, Some(0.0))
}

/// Forgets the state of [`appear`] and [`value`] for `id`.
pub fn reset(ctx: &Context, id: Id) {
    ctx.data_mut(|d| d.remove::<Tween>(id.with("pdfxray-motion")));
}

/// The intensity of a flash (1.0 falling to 0.0 over `timing`) that starts when `key` changes
/// between calls. The first call is quiet. Hash the displayed value into `key`.
pub fn flash(ctx: &Context, id: Id, key: u64, timing: Timing) -> f32 {
    let now = ctx.input(|i| i.time);
    let key_id = id.with("pdfxray-flash");
    let last: Option<(u64, f64)> = ctx.data(|d| d.get_temp(key_id));
    let started = match last {
        None => f64::NEG_INFINITY,
        Some((previous, at)) if previous == key => at,
        Some(_) => now,
    };
    ctx.data_mut(|d| d.insert_temp(key_id, (key, started)));
    if reduce_motion() {
        return 0.0;
    }
    let tween = Tween {
        from: 1.0,
        to: 0.0,
        start: started,
    };
    let progress = tween.progress(now, timing);
    if progress < 1.0 {
        ctx.request_repaint();
    }
    tween.at(now, timing)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use egui::{Context, Id, RawInput};
    use pretty_assertions::assert_eq;

    use super::*;

    /// Runs one frame at time `t` seconds and returns what `f` computes in it.
    fn frame<R>(ctx: &Context, t: f64, f: impl FnOnce(&Context) -> R) -> R {
        let mut f = Some(f);
        let mut out = None;
        let mut output = ctx.run_ui(
            RawInput {
                time: Some(t),
                ..RawInput::default()
            },
            |ui| {
                if let Some(f) = f.take() {
                    out = Some(f(ui.ctx()));
                }
            },
        );
        output.textures_delta.clear();
        out.expect("the frame ran")
    }

    // Reduce motion is a process-wide flag; these tests share it, so they set it themselves and
    // run as one test where the order matters.
    #[test]
    fn values_tween_eased_retarget_from_where_they_are_and_jump_under_reduce_motion() {
        set_reduce_motion(false);
        let ctx = Context::default();
        let id = Id::unique("zoom");

        // The first call is the target; nothing animates yet.
        assert_eq!(frame(&ctx, 0.0, |c| value(c, id, 1.0, ZOOM)), 1.0);
        // A new target starts from the old value...
        assert_eq!(frame(&ctx, 1.0, |c| value(c, id, 2.0, ZOOM)), 1.0);
        // ...is half way after half the time on a symmetric curve, and reaches the end.
        let halfway = frame(&ctx, 1.0 + f64::from(ZOOM.secs) / 2.0, |c| {
            value(c, id, 2.0, ZOOM)
        });
        assert!((halfway - 1.5).abs() < 1e-4, "{halfway}");
        assert_eq!(frame(&ctx, 2.0, |c| value(c, id, 2.0, ZOOM)), 2.0);

        // Interrupted a quarter in, it turns around from the value shown, not from the end.
        frame(&ctx, 3.0, |c| value(c, id, 4.0, ZOOM));
        let shown = frame(&ctx, 3.0 + f64::from(ZOOM.secs) / 4.0, |c| {
            value(c, id, 4.0, ZOOM)
        });
        assert!(shown > 2.0 && shown < 3.0, "{shown}");
        let back = frame(&ctx, 3.0 + f64::from(ZOOM.secs) / 4.0, |c| {
            value(c, id, 2.0, ZOOM)
        });
        assert!(
            (back - shown).abs() < 1e-4,
            "retargeting must not jump: {back} vs {shown}"
        );
        let later = frame(&ctx, 3.0 + f64::from(ZOOM.secs) / 2.0, |c| {
            value(c, id, 2.0, ZOOM)
        });
        assert!(later < shown && later >= 2.0, "{later} should head back to 2.0");

        // Appear: 0 when first shown, 1 after the timing, and again after a reset.
        let tip = Id::unique("tip");
        assert_eq!(frame(&ctx, 5.0, |c| appear(c, tip, POPOVER)), 0.0);
        let mid = frame(&ctx, 5.0 + f64::from(POPOVER.secs) / 2.0, |c| {
            appear(c, tip, POPOVER)
        });
        assert!(mid > 0.0 && mid < 1.0, "{mid}");
        assert_eq!(frame(&ctx, 6.0, |c| appear(c, tip, POPOVER)), 1.0);
        frame(&ctx, 6.1, |c| reset(c, tip));
        assert_eq!(frame(&ctx, 7.0, |c| appear(c, tip, POPOVER)), 0.0);

        // A flash is quiet at first, 1 when the key changes, and gone after its timing.
        let cell = Id::unique("cell");
        assert_eq!(frame(&ctx, 8.0, |c| flash(c, cell, 1, FLASH)), 0.0);
        assert_eq!(frame(&ctx, 9.0, |c| flash(c, cell, 2, FLASH)), 1.0);
        let fading = frame(&ctx, 9.0 + f64::from(FLASH.secs) / 2.0, |c| {
            flash(c, cell, 2, FLASH)
        });
        assert!(fading > 0.0 && fading < 1.0, "{fading}");
        assert_eq!(frame(&ctx, 10.0, |c| flash(c, cell, 2, FLASH)), 0.0);

        // Reduce motion: every animation is at its end at once.
        set_reduce_motion(true);
        assert!(reduce_motion());
        assert_eq!(frame(&ctx, 11.0, |c| value(c, id, 9.0, ZOOM)), 9.0);
        assert_eq!(
            frame(&ctx, 11.0, |c| appear(c, Id::unique("fresh"), POPOVER)),
            1.0
        );
        assert_eq!(
            frame(&ctx, 11.0, |c| flag(c, Id::unique("f"), true, SELECTION)),
            1.0
        );
        assert_eq!(
            frame(&ctx, 11.0, |c| flag(c, Id::unique("f"), false, SELECTION)),
            0.0
        );
        assert_eq!(frame(&ctx, 12.0, |c| flash(c, cell, 3, FLASH)), 0.0);
        set_reduce_motion(false);
        assert!(!reduce_motion(), "smoke: the setter wins over the startup read");
    }

    #[test]
    fn easing_curves_fix_both_ends_and_do_not_overshoot() {
        for curve in [easing::linear, easing::ease_out, easing::ease_in_out] {
            assert_eq!((curve(0.0), curve(1.0)), (0.0, 1.0));
            let mut last = 0.0;
            for step in 0..=20 {
                let y = curve(pdfxray_core::numeric::f32_from_u32(step) / 20.0);
                assert!((0.0..=1.0).contains(&y), "{y}");
                assert!(y >= last, "monotonic: {y} after {last}");
                last = y;
            }
        }
        // Ease-out is ahead of linear early on, ease-in-out is behind it.
        assert!(easing::ease_out(0.25) > 0.25);
        assert!(easing::ease_in_out(0.25) < 0.25);
    }

    #[test]
    fn the_setting_comes_from_the_environment_first_then_macos_then_false() {
        assert!(resolve(Some("1"), None));
        assert!(!resolve(Some("0"), Some("1\n")), "the variable beats the system");
        assert!(resolve(None, Some("1\n")));
        assert!(!resolve(None, Some("0\n")));
        assert!(!resolve(None, None), "a failed read is off");
        assert!(
            resolve(Some("maybe"), Some("1")),
            "garbage falls through to the system"
        );
        assert!(!resolve(Some(""), None));
        // Smoke: the durations of the design.
        assert_eq!(
            [SELECTION, POPOVER, ZOOM, PAINT_STEP, PATH, FLASH].map(|t| t.secs),
            [0.12, 0.14, 0.22, 0.09, 0.3, 0.4]
        );
    }
}
