//! Sparkle, the updater, loaded at run time from the app bundle's `Contents/Frameworks`.
//!
//! Nothing links against Sparkle: `cargo xtask bundle` copies `Sparkle.framework` into the
//! release app, and [`start`] loads it with `NSBundle`, looks its controller class up by name and
//! creates an `SPUStandardUpdaterController`, which starts the updater (automatic checks per the
//! `SU*` keys of `Info.plist`, the update dialogs, installing and relaunching). A build without
//! the framework (`cargo run`, tests) simply has no updater.

use std::cell::RefCell;
use std::ffi::CStr;
use std::path::PathBuf;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_foundation::{NSBundle, NSError, NSString};

use crate::UpdaterError;

/// The framework's directory name inside `Contents/Frameworks`.
const FRAMEWORK: &str = "Sparkle.framework";

/// The class that owns Sparkle's updater and its standard user interface.
const CONTROLLER_CLASS: &CStr = c"SPUStandardUpdaterController";

thread_local! {
    /// The controller once [`start`] created it; it lives as long as the process. Only the main
    /// thread creates and uses it.
    static CONTROLLER: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

/// `Contents/Frameworks/Sparkle.framework` of the running app, when it is there.
pub(crate) fn bundled_framework() -> Option<PathBuf> {
    let frameworks = NSBundle::mainBundle().privateFrameworksPath()?;
    let path = PathBuf::from(frameworks.to_string()).join(FRAMEWORK);
    path.is_dir().then_some(path)
}

pub(crate) fn start() -> Result<(), UpdaterError> {
    if MainThreadMarker::new().is_none() {
        return Err(UpdaterError::NotMainThread);
    }
    if CONTROLLER.with(|slot| slot.borrow().is_some()) {
        return Ok(());
    }
    let path = bundled_framework().ok_or(UpdaterError::NotBundled)?;
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(&path.to_string_lossy()))
        .ok_or_else(|| UpdaterError::Load(format!("{} is not a bundle", path.display())))?;
    // SAFETY: loading a bundle runs its initialisers. This one is the Sparkle framework inside the
    // app's own signed bundle; the hardened runtime's library validation maps it only when it is
    // signed by the same team as the app.
    unsafe { bundle.loadAndReturnError() }.map_err(|error| UpdaterError::Load(load_failure(&error)))?;

    let class = AnyClass::get(CONTROLLER_CLASS).ok_or(UpdaterError::Incompatible)?;
    let init = sel!(initWithStartingUpdater:updaterDelegate:userDriverDelegate:);
    if !class.responds_to(init) || !class.responds_to(sel!(checkForUpdates:)) {
        return Err(UpdaterError::Incompatible);
    }
    // SAFETY: `alloc` on a class returns a new, uninitialised instance of it.
    let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
    // SAFETY: Sparkle 2 declares `- (instancetype)initWithStartingUpdater:(BOOL)startUpdater
    // updaterDelegate:(nullable id)updaterDelegate userDriverDelegate:(nullable id)userDriverDelegate`
    // on this class (checked above that it responds); both delegates may be nil. We are on the
    // main thread, where Sparkle wants to be set up.
    let controller: Option<Retained<AnyObject>> = unsafe {
        msg_send![
            allocated,
            initWithStartingUpdater: true,
            updaterDelegate: None::<&AnyObject>,
            userDriverDelegate: None::<&AnyObject>
        ]
    };
    let controller = controller.ok_or(UpdaterError::Incompatible)?;
    CONTROLLER.with(|slot| slot.replace(Some(controller)));
    Ok(())
}

/// Why a bundle did not load: `dlopen`'s reason (`NSDebugDescription`, for example a refusal by
/// library validation), else the localized description, which only says it could not be loaded.
fn load_failure(error: &NSError) -> String {
    error
        .userInfo()
        .objectForKey(&NSString::from_str("NSDebugDescription"))
        .and_then(|reason| reason.downcast::<NSString>().ok())
        .map_or_else(
            || error.localizedDescription().to_string(),
            |reason| reason.to_string(),
        )
}

pub(crate) fn check_for_updates() -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    // A clone, so no borrow is held while Sparkle runs.
    let Some(controller) = CONTROLLER.with(|slot| slot.borrow().clone()) else {
        return false;
    };
    // SAFETY: `- (IBAction)checkForUpdates:(nullable id)sender` is a method of
    // `SPUStandardUpdaterController` (checked in `start`): one object argument that may be nil, no
    // return value. Menu actions run on the main thread, as here.
    let () = unsafe { msg_send![&*controller, checkForUpdates: None::<&AnyObject>] };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_binary_has_no_bundled_framework_and_no_updater() {
        // `cargo test` runs a bare executable, not an app with `Contents/Frameworks`.
        assert_eq!(bundled_framework(), None);
        assert!(!check_for_updates());
        // smoke: the class is not registered either, since nothing loaded Sparkle
        assert!(AnyClass::get(CONTROLLER_CLASS).is_none());
    }
}
