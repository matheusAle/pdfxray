//! The `AppKit` side: receiving opened files through the application delegate, and the recent
//! documents. All of pdfxray's `unsafe` code is in this file.
//!
//! winit 0.30 installs its own `NSApplicationDelegate` (`WinitApplicationDelegate`) when the event
//! loop is built, and panics if the delegate is replaced by another object. So instead of setting
//! a delegate of our own, [`install_open_handler`] adds the one method `AppKit` needs,
//! `application:openURLs:`, to the class of the delegate that is installed, then sets that same
//! delegate again so `AppKit` notices the method.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::ptr;

use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{MainThreadMarker, ffi, sel};
use objc2_app_kit::{NSApplication, NSDocumentController};
use objc2_foundation::{NSArray, NSURL};

/// What the delegate calls with the opened files.
type OnOpen = Box<dyn Fn(Vec<PathBuf>)>;

/// The signature of `application:openURLs:` as a C function: receiver, selector, application,
/// URLs; no return value.
type OpenUrls = extern "C-unwind" fn(&AnyObject, Sel, &NSApplication, &NSArray<NSURL>);

/// Its Objective-C type encoding: `void` return, `id self`, `SEL _cmd`, two object arguments.
const OPEN_URLS_TYPES: &std::ffi::CStr = c"v@:@@";

thread_local! {
    /// The handler [`open_urls`] calls. Only the main thread installs it, and `AppKit` only calls
    /// the delegate there.
    static ON_OPEN: RefCell<Option<OnOpen>> = const { RefCell::new(None) };
}

/// `application:openURLs:`, added to the installed delegate's class.
extern "C-unwind" fn open_urls(
    _delegate: &AnyObject,
    _cmd: Sel,
    _app: &NSApplication,
    urls: &NSArray<NSURL>,
) {
    let paths = file_paths(urls);
    if paths.is_empty() {
        return;
    }
    ON_OPEN.with(|slot| {
        if let Some(on_open) = slot.borrow().as_ref() {
            on_open(paths);
        }
    });
}

pub(crate) fn install_open_handler(on_open: OnOpen) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else {
        return false;
    };
    let object: &AnyObject = (*delegate).as_ref();
    let class: &AnyClass = object.class();
    let selector = sel!(application:openURLs:);
    // SAFETY: `open_urls` has exactly the C ABI of an Objective-C method with the signature
    // declared by `OPEN_URLS_TYPES` (`self`, `_cmd`, two object pointers, no return value);
    // `Imp` is the type-erased function pointer the runtime stores for any method.
    let imp = unsafe { std::mem::transmute::<OpenUrls, Imp>(open_urls) };
    match class.instance_method(selector) {
        // Installed by an earlier call: only the handler changes.
        Some(method) if std::ptr::fn_addr_eq(method.implementation(), imp) => {}
        // The delegate answers it already in its own way; leave it alone.
        Some(_) => return false,
        None => {
            // SAFETY: `class` is a registered class (the class of a live object), the selector and
            // the type encoding match `imp`'s signature, and the encoding is a valid C string.
            // `class_addMethod` only adds a method that does not exist yet.
            let added = unsafe {
                ffi::class_addMethod(
                    ptr::from_ref(class).cast_mut(),
                    selector,
                    imp,
                    OPEN_URLS_TYPES.as_ptr(),
                )
            };
            if !added.as_bool() {
                return false;
            }
        }
    }
    ON_OPEN.with(|slot| slot.replace(Some(on_open)));
    // `AppKit` reads which optional methods a delegate implements when it is set.
    app.setDelegate(None);
    app.setDelegate(Some(&delegate));
    true
}

pub(crate) fn note_recent_document(path: &Path) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(url) = NSURL::from_file_path(path) else {
        return;
    };
    NSDocumentController::sharedDocumentController(mtm).noteNewRecentDocumentURL(&url);
}

/// The file system paths of the `file:` URLs in `urls`; other schemes are left out.
fn file_paths(urls: &NSArray<NSURL>) -> Vec<PathBuf> {
    urls.iter()
        .filter(|url| url.isFileURL())
        .filter_map(|url| url.to_file_path())
        .collect()
}

#[cfg(test)]
mod tests {
    use objc2_foundation::NSString;

    use super::*;

    #[test]
    fn file_urls_become_paths_and_other_schemes_are_left_out() {
        let file = NSURL::from_file_path("/Users/someone/My Files/report é.pdf").expect("file url");
        let web = NSURL::URLWithString(&NSString::from_str("https://example.com/a.pdf")).expect("web url");
        let other = NSURL::from_file_path("/tmp/b.pdf").expect("file url");
        let urls = NSArray::from_retained_slice(&[file, web, other]);

        assert_eq!(
            file_paths(&urls),
            [
                PathBuf::from("/Users/someone/My Files/report é.pdf"),
                PathBuf::from("/tmp/b.pdf"),
            ]
        );
        // smoke: nothing in, nothing out
        assert_eq!(file_paths(&NSArray::new()), Vec::<PathBuf>::new());
    }
}
